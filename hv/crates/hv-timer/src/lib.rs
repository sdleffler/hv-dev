//! Timing and measurement functionality.
//!
//! This crate is a modified verion of `ggez::timer`. See the license information below in the
//! source file.
//!
//! As of the current version, Heavy relies on the `miniquad` crate for an update loop, and the
//! `miniquad` event loop is generally capped at 60FPS. However, on some systems, this vsync-capping
//! doesn't function properly or doesn't exist, and the event loop will spiral out of control. In
//! this event or in the event that `miniquad` or whatever backend we're using in the future gains
//! the ability to run without a framerate cap, then this module can be used to restrict framerate
//! (and/or do other timing corrections.)
//!
//! Generally it is advisable to use a fixed timestep even if your game is capable of using a
//! variable timestep and running at over 60FPS; things like physics engines can become wildly
//! unstable if their timestep/delta-t is varying too much between frames. For a more detailed
//! tutorial on how to handle frame timings in games, see
//! <http://gafferongames.com/game-physics/fix-your-timestep/>

/*
 * The MIT License (MIT)
 *
 * Copyright (c) 2016-2017 ggez-dev
 *
 * Permission is hereby granted, free of charge, to any person obtaining a copy
 * of this software and associated documentation files (the "Software"), to deal
 * in the Software without restriction, including without limitation the rights
 * to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
 * copies of the Software, and to permit persons to whom the Software is
 * furnished to do so, subject to the following conditions:
 *
 * The above copyright notice and this permission notice shall be included in all
 * copies or substantial portions of the Software.
 *
 * THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
 * IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
 * FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
 * AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
 * LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
 * OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
 * SOFTWARE.
 */

use std::cmp;
use std::f64;
use std::thread;
use std::time;

/// A simple buffer that fills
/// up to a limit and then holds the last
/// N items that have been inserted into it,
/// overwriting old ones in a round-robin fashion.
///
/// It's not quite a ring buffer 'cause you can't
/// remove items from it, it just holds the last N
/// things.
#[derive(Debug, Clone)]
struct LogBuffer<T>
where
    T: Clone,
{
    head: usize,
    size: usize,
    /// The number of actual samples inserted, used for
    /// smarter averaging.
    samples: usize,
    contents: Vec<T>,
}

impl<T> LogBuffer<T>
where
    T: Clone + Copy,
{
    fn new(size: usize, init_val: T) -> LogBuffer<T> {
        LogBuffer {
            head: 0,
            size,
            contents: vec![init_val; size],
            // Never divide by 0
            samples: 1,
        }
    }

    /// Pushes a new item into the `LogBuffer`, overwriting
    /// the oldest item in it.
    fn push(&mut self, item: T) {
        self.head = (self.head + 1) % self.contents.len();
        self.contents[self.head] = item;
        self.size = cmp::min(self.size + 1, self.contents.len());
        self.samples += 1;
    }

    /// Returns a slice pointing at the contents of the buffer.
    /// They are in *no particular order*, and if not all the
    /// slots are filled, the empty slots will be present but
    /// contain the initial value given to [`new()`](#method.new).
    ///
    /// We're only using this to log FPS for a short time,
    /// so we don't care for the second or so when it's inaccurate.
    fn contents(&self) -> &[T] {
        if self.samples > self.size {
            &self.contents
        } else {
            &self.contents[..self.samples]
        }
    }

    /// Returns the most recent value in the buffer.
    fn latest(&self) -> T {
        self.contents[self.head]
    }
}

/// A structure that contains our time-tracking state.
#[derive(Debug)]
pub struct TimeContext {
    init_instant: time::Instant,
    last_instant: time::Instant,
    frame_durations: LogBuffer<time::Duration>,
    residual_update_dt: time::Duration,
    frame_count: usize,
}

// How many frames we log update times for.
const TIME_LOG_FRAMES: usize = 200;

impl TimeContext {
    /// Creates a new `TimeContext` and initializes the start to this instant.
    pub fn new() -> TimeContext {
        let initial_dt = time::Duration::from_millis(16);
        TimeContext {
            init_instant: time::Instant::now(),
            last_instant: time::Instant::now(),
            frame_durations: LogBuffer::new(TIME_LOG_FRAMES, initial_dt),
            residual_update_dt: time::Duration::from_secs(0),
            frame_count: 0,
        }
    }

    /// Update the state of the `TimeContext` to record that
    /// another frame has taken place.  Necessary for the FPS
    /// tracking and [`check_update_time()`](fn.check_update_time.html)
    /// functions to work.
    pub fn tick(&mut self) {
        let now = time::Instant::now();
        let time_since_last = now - self.last_instant;
        self.frame_durations.push(time_since_last);
        self.last_instant = now;
        self.frame_count += 1;

        self.residual_update_dt += time_since_last;
    }
}

impl Default for TimeContext {
    fn default() -> Self {
        Self::new()
    }
}

impl TimeContext {
    /// Get the time between the start of the last frame and the current one;
    /// in other words, the length of the last frame.
    pub fn delta(&self) -> time::Duration {
        self.frame_durations.latest()
    }

    /// Gets the average time of a frame, averaged
    /// over the last 200 frames.
    pub fn average_delta(&self) -> time::Duration {
        let sum: time::Duration = self.frame_durations.contents().iter().sum();
        // If our buffer is actually full, divide by its size.
        // Otherwise divide by the number of samples we've added
        if self.frame_durations.samples > self.frame_durations.size {
            sum / (self.frame_durations.size as u32)
        } else {
            sum / (self.frame_durations.samples as u32)
        }
    }

    /// Gets the FPS of the game, averaged over the last
    /// 200 frames.
    pub fn fps(&self) -> f64 {
        let duration_per_frame = self.average_delta();
        let seconds_per_frame = duration_to_f64(duration_per_frame);
        1.0 / seconds_per_frame
    }

    /// Returns the time since the game was initialized,
    /// as reported by the system clock.
    pub fn time_since_start(&self) -> time::Duration {
        time::Instant::now() - self.init_instant
    }

    /// Check whether or not the desired amount of time has elapsed
    /// since the last frame.
    ///
    /// This function will return true if the time since the last
    /// [`tick()`](TimeContext::tick)
    /// call has been equal to or greater to the update FPS indicated by
    /// the `target_fps`.  It keeps track of fractional frames, so if you
    /// want 60 fps (16.67 ms/frame) and the game stutters so that there
    /// is 40 ms between `update()` calls, this will return `true` twice
    /// in a row even in the same frame, then taking into account the
    /// residual 6.67 ms to catch up to the next frame before returning
    /// `true` again.
    ///
    /// The intention is to for it to be called in a while loop
    /// in your `update()` callback:
    ///
    /// ```rust
    /// # use hv_core::{prelude::*, timer::{self, TimeContext}};
    /// # fn update_game_physics() -> Result<()> { Ok(()) }
    /// # struct State;
    /// # impl State {
    /// fn update(&mut self, ctx: &mut TimeContext) -> Result<()> {
    ///     ctx.tick();
    ///     while ctx.check_update_time(60) {
    ///         update_game_physics()?;
    ///     }
    ///     Ok(())
    /// }
    /// # }
    /// ```
    pub fn check_update_time(&mut self, target_fps: u32) -> bool {
        let target_dt = fps_as_duration(target_fps);
        if self.residual_update_dt > target_dt {
            self.residual_update_dt -= target_dt;
            true
        } else {
            false
        }
    }

    /// This is a variant of `check_update_time` which intends you to pass an iteration
    /// counter. If the iteration counter is zero, it will do an update regardless of
    /// whether there's enough remaining time, and set the residual delta time to zero.
    /// This helps avoid cascading stutters where the game performs no updates one frame
    /// and then many the next.
    ///
    /// ```rust
    /// # use hv_core::{prelude::*, timer::{self, TimeContext}};
    /// # fn update_game_physics() -> Result<()> { Ok(()) }
    /// # struct State;
    /// # impl State {
    /// fn update(&mut self, ctx: &mut TimeContext) -> Result<()> {
    ///     ctx.tick();
    ///     let mut counter = 0;
    ///     while ctx.check_update_time_forced(60, &mut counter) {
    ///         update_game_physics()?;
    ///     }
    ///     Ok(())
    /// }
    /// # }
    /// ```
    pub fn check_update_time_forced(&mut self, target_fps: u32, iteration: &mut u32) -> bool {
        let target_dt = fps_as_duration(target_fps);
        if self.residual_update_dt > target_dt
            || (*iteration == 0 && self.fps() < 2. * target_fps as f64)
        {
            *iteration += 1;
            self.residual_update_dt = self
                .residual_update_dt
                .checked_sub(target_dt)
                .unwrap_or_default();
            true
        } else {
            false
        }
    }

    /// Returns the fractional amount of a frame not consumed
    /// by  [`check_update_time()`](fn.check_update_time.html).
    /// For example, if the desired
    /// update frame time is 40 ms (25 fps), and 45 ms have
    /// passed since the last frame, [`check_update_time()`](fn.check_update_time.html)
    /// will return `true` and `remaining_update_time()` will
    /// return 5 ms -- the amount of time "overflowing" from one
    /// frame to the next.
    ///
    /// The intention is for it to be called in your
    /// [`draw()`](../event/trait.EventHandler.html#tymethod.draw) callback
    /// to interpolate physics states for smooth rendering.
    /// (see <http://gafferongames.com/game-physics/fix-your-timestep/>)
    pub fn remaining_update_time(&self) -> time::Duration {
        self.residual_update_dt
    }

    /// Gets the number of times the game has gone through its event loop.
    ///
    /// Specifically, the number of times that [`TimeContext::tick()`](struct.TimeContext.html#method.tick)
    /// has been called by it.
    pub fn ticks(&self) -> usize {
        self.frame_count
    }
}

/// Pauses the current thread for the target duration.
/// Just calls [`std::thread::sleep()`](https://doc.rust-lang.org/std/thread/fn.sleep.html)
/// so it's as accurate as that is (which is usually not very).
pub fn sleep(duration: time::Duration) {
    thread::sleep(duration);
}

/// Yields the current timeslice to the OS.
///
/// This just calls [`std::thread::yield_now()`](https://doc.rust-lang.org/std/thread/fn.yield_now.html)
/// but it's handy to have here.
pub fn yield_now() {
    thread::yield_now();
}

/// A convenience function to convert a Rust `Duration` type
/// to a (less precise but more useful) `f64`.
///
/// Does not make sure that the `Duration` is within the bounds
/// of the `f64`.
pub fn duration_to_f64(d: time::Duration) -> f64 {
    let seconds = d.as_secs() as f64;
    let nanos = f64::from(d.subsec_nanos());
    seconds + (nanos * 1e-9)
}

/// A convenience function to create a Rust `Duration` type
/// from a (less precise but more useful) `f64`.
///
/// Only handles positive numbers correctly.
pub fn f64_to_duration(t: f64) -> time::Duration {
    debug_assert!(t > 0.0, "f64_to_duration passed a negative number!");
    let seconds = t.trunc();
    let nanos = t.fract() * 1e9;
    time::Duration::new(seconds as u64, nanos as u32)
}

/// A convenience function to convert a Rust `Duration` type
/// to a (less precise but more useful) `f32`.
///
/// Does not make sure that the `Duration` is within the bounds
/// of the `f32`.
pub fn duration_to_f32(d: time::Duration) -> f32 {
    let seconds = d.as_secs() as f32;
    let nanos = f32::from(d.subsec_nanos() as u16);
    seconds + (nanos * 1e-9)
}

/// A convenience function to create a Rust `Duration` type
/// from a (less precise but more useful) `f32`.
///
/// Only handles positive numbers correctly.
pub fn f32_to_duration(t: f32) -> time::Duration {
    debug_assert!(t > 0.0, "f64_to_duration passed a negative number!");
    let seconds = t.trunc();
    let nanos = t.fract() * 1e9;
    time::Duration::new(seconds as u64, nanos as u32)
}

/// Returns a `Duration` representing how long each
/// frame should be to match the given fps.
///
/// Approximately.
fn fps_as_duration(fps: u32) -> time::Duration {
    let target_dt_seconds = 1.0 / f64::from(fps);
    f64_to_duration(target_dt_seconds)
}
