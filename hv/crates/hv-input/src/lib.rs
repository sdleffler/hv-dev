//! Abstractions for handling input and creating input key/button/etc. bindings.
//!
//! Heavily based on the `ggez-goodies` crate's `input.rs` module; please see the source of this
//! file for the license notification.
//!
//! An abstraction over input bindings, so that you can handle inputs in a platform- and windowing
//! library-independent fashion. Supports keyboard keycodes and "scancodes", gamepad buttons and
//! axes, and mouse buttons and movement.

/*
 * MIT License
 *
 * Copyright (c) 2016-2018 the ggez developers
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

use std::{hash::Hash, path::PathBuf};

use hashbrown::HashMap;
use hv_alchemy::Type;
use hv_lua::{FromLua, ToLua, UserData, UserDataMethods};
use hv_math::{Point2, Vector2};
use serde::*;

#[cfg(feature = "glfw")]
mod glfw;

pub trait Mappable: Eq + Hash + Clone {}
impl<T: Eq + Hash + Clone> Mappable for T {}

pub trait LuaMappable: Mappable + for<'lua> FromLua<'lua> + for<'lua> ToLua<'lua> {}
impl<T: Mappable + for<'lua> FromLua<'lua> + for<'lua> ToLua<'lua>> LuaMappable for T {}

/// Supported key codes.
#[allow(missing_docs)]
#[derive(
    Debug,
    Copy,
    Clone,
    PartialEq,
    Hash,
    Eq,
    strum::EnumString,
    strum::EnumIter,
    Serialize,
    Deserialize,
)]
#[strum(ascii_case_insensitive)]
#[repr(u32)]
pub enum Key {
    Space,
    Apostrophe,
    Comma,
    Minus,
    Period,
    Slash,
    Num0,
    Num1,
    Num2,
    Num3,
    Num4,
    Num5,
    Num6,
    Num7,
    Num8,
    Num9,
    Semicolon,
    Equal,
    A,
    B,
    C,
    D,
    E,
    F,
    G,
    H,
    I,
    J,
    K,
    L,
    M,
    N,
    O,
    P,
    Q,
    R,
    S,
    T,
    U,
    V,
    W,
    X,
    Y,
    Z,
    LeftBracket,
    Backslash,
    RightBracket,
    GraveAccent,
    World1,
    World2,
    Escape,
    Enter,
    Tab,
    Backspace,
    Insert,
    Delete,
    Right,
    Left,
    Down,
    Up,
    PageUp,
    PageDown,
    Home,
    End,
    CapsLock,
    ScrollLock,
    NumLock,
    PrintScreen,
    Pause,
    F1,
    F2,
    F3,
    F4,
    F5,
    F6,
    F7,
    F8,
    F9,
    F10,
    F11,
    F12,
    F13,
    F14,
    F15,
    F16,
    F17,
    F18,
    F19,
    F20,
    F21,
    F22,
    F23,
    F24,
    F25,
    Kp0,
    Kp1,
    Kp2,
    Kp3,
    Kp4,
    Kp5,
    Kp6,
    Kp7,
    Kp8,
    Kp9,
    KpDecimal,
    KpDivide,
    KpMultiply,
    KpSubtract,
    KpAdd,
    KpEnter,
    KpEqual,
    LeftShift,
    LeftControl,
    LeftAlt,
    LeftSuper,
    RightShift,
    RightControl,
    RightAlt,
    RightSuper,
    Menu,
    Unknown,
}

#[derive(
    Debug, Copy, Clone, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize,
)]
pub struct KeyMods {
    pub shift: bool,
    pub ctrl: bool,
    pub alt: bool,
    pub cmd: bool,
    pub caps_lock: bool,
    pub num_lock: bool,
}

#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    strum::EnumString,
    strum::EnumIter,
    Serialize,
    Deserialize,
)]
#[repr(i32)]
pub enum MouseButton {
    Button1,
    Button2,
    Button3,
    Button4,
    Button5,
    Button6,
    Button7,
    Button8,
    Unknown,
}

#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    strum::EnumString,
    strum::EnumIter,
    Serialize,
    Deserialize,
)]
#[allow(missing_docs)]
pub enum GamepadButton {
    /// The south button of the typical quadruplet.
    A,
    /// East.
    B,
    /// West.
    X,
    /// North.
    Y,
    C,
    Z,
    LeftTrigger,
    LeftTrigger2,
    RightTrigger,
    RightTrigger2,
    Select,
    Start,
    Mode,
    LeftThumb,
    RightThumb,
    DPadUp,
    DPadDown,
    DPadLeft,
    DPadRight,
    Unknown,
}

#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    strum::EnumString,
    strum::EnumIter,
    Serialize,
    Deserialize,
)]
#[allow(missing_docs)]
pub enum GamepadAxis {
    LeftStickX,
    LeftStickY,
    LeftZ,
    RightStickX,
    RightStickY,
    RightZ,
    DPadX,
    DPadY,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum AxisDirection {
    Positive,
    Negative,
}

impl AxisDirection {
    pub fn pressed(self, pressed: bool) -> f32 {
        pressed.then(|| self.sign()).unwrap_or(0.)
    }

    pub fn sign(self) -> f32 {
        match self {
            Self::Positive => 1.0,
            Self::Negative => -1.0,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ScrollAxis {
    Horizontal,
    Vertical,
}

#[derive(Debug, Copy, Clone, PartialEq, Serialize, Deserialize)]
pub enum InputEffect<Axes, Buttons>
where
    Axes: Mappable,
    Buttons: Mappable,
{
    /// An input effect on an axis, with an associated direction.
    Axis(Axes, AxisDirection),
    /// An input effect setting the state of a button (up or down.) Second parameter affects the
    /// sign which is considered "pressed" if an axis is mapped to a button.
    Button(Buttons, AxisDirection),
}

impl<Axes, Buttons> InputEffect<Axes, Buttons>
where
    Axes: Mappable,
    Buttons: Mappable,
{
    pub fn to_event_with_button_state(self, state: bool) -> InputEvent<Axes, Buttons> {
        match self {
            InputEffect::Axis(axis, direction) => InputEvent::Axis(axis, direction.pressed(state)),
            InputEffect::Button(button, _) => InputEvent::Button {
                button,
                state,
                repeat: false,
            },
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, PartialOrd, Serialize, Deserialize)]
pub enum InputEvent<Axes, Buttons>
where
    Axes: Mappable,
    Buttons: Mappable,
{
    /// Indicates the state of an axis.
    Axis(Axes, f32),
    /// Indicates the state of a given button.
    Button {
        button: Buttons,
        state: bool,
        repeat: bool,
    },
    /// Indicates the state of the cursor.
    Cursor(Point2<f32>),
}

#[derive(Debug, Hash, Eq, PartialEq, Copy, Clone)]
enum InputType {
    KeyCode(Key),
    ScanCode(Key),
    GamepadButton(GamepadButton),
    GamepadAxis(GamepadAxis),
    MouseButton(MouseButton),
    MouseScroll(ScrollAxis),
}

#[derive(Debug, Hash, PartialEq, Eq, Copy, Clone)]
pub enum GenericAxis {
    Gamepad(GamepadAxis),
    Mouse(ScrollAxis),
}

#[derive(Debug, Hash, PartialEq, Eq, Copy, Clone)]
pub enum GenericButton {
    KeyCode(Key, KeyMods),
    ScanCode(Key, KeyMods),
    Gamepad(GamepadButton),
    Mouse(MouseButton, KeyMods),
}

#[derive(Debug, Copy, Clone)]
struct CursorState {
    // Where the cursor currently is.
    position: Point2<f32>,
    // Where the cursor was last frame.
    last_position: Point2<f32>,
    // The difference between the current position and the position last update.
    delta: Vector2<f32>,
}

impl Default for CursorState {
    fn default() -> Self {
        Self {
            position: Point2::origin(),
            last_position: Point2::origin(),
            delta: Vector2::zeros(),
        }
    }
}

#[derive(Debug, Copy, Clone)]
struct AxisState {
    // Where the axis currently is, in [-1, 1]
    position: f32,
    // Where the axis is moving towards.  Possible values are -1, 0, +1 (or a continuous range for
    // analog devices I guess)
    direction: f32,
    // Speed in units per second that the axis moves towards the target value.
    acceleration: f32,
    // Speed in units per second that the axis will fall back toward 0 if the input stops.
    gravity: f32,
}

impl Default for AxisState {
    fn default() -> Self {
        AxisState {
            position: 0.0,
            direction: 0.0,
            acceleration: 16.0,
            gravity: 12.0,
        }
    }
}

#[derive(Debug, Copy, Clone, Default)]
struct ButtonState {
    pressed: bool,
    pressed_last_frame: bool,
    repeat: bool,
}

pub struct InputBinding<Axes, Buttons>
where
    Axes: Mappable,
    Buttons: Mappable,
{
    bindings: HashMap<InputType, InputEffect<Axes, Buttons>>,
}

impl<Axes, Buttons> Default for InputBinding<Axes, Buttons>
where
    Axes: Hash + Eq + Clone,
    Buttons: Hash + Eq + Clone,
{
    fn default() -> Self {
        Self::new()
    }
}

impl<Axes, Buttons> InputBinding<Axes, Buttons>
where
    Axes: Hash + Eq + Clone,
    Buttons: Hash + Eq + Clone,
{
    /// Create an empty set of input bindings.
    pub fn new() -> Self {
        InputBinding {
            bindings: HashMap::new(),
        }
    }

    /// Adds a key binding connecting the given keycode to the given logical axis.
    pub fn bind_keycode_to_axis(
        mut self,
        keycode: Key,
        axis: Axes,
        direction: AxisDirection,
    ) -> Self {
        self.bindings.insert(
            InputType::KeyCode(keycode),
            InputEffect::Axis(axis, direction),
        );
        self
    }

    /// Adds a key binding connecting the given scancode (layout-independent key identifier) to the
    /// given logical axis.
    pub fn bind_scancode_to_axis(
        mut self,
        keycode: Key,
        axis: Axes,
        direction: AxisDirection,
    ) -> Self {
        self.bindings.insert(
            InputType::ScanCode(keycode),
            InputEffect::Axis(axis, direction),
        );
        self
    }

    /// Adds a key binding connecting the given keycode to the given logical button.
    pub fn bind_keycode_to_button(mut self, keycode: Key, button: Buttons) -> Self {
        self.bindings.insert(
            InputType::KeyCode(keycode),
            InputEffect::Button(button, AxisDirection::Positive),
        );
        self
    }

    /// Adds a key binding connecting the given scancode (layout-independent key identifier) to the
    /// given logical button.
    pub fn bind_scancode_to_button(mut self, keycode: Key, button: Buttons) -> Self {
        self.bindings.insert(
            InputType::ScanCode(keycode),
            InputEffect::Button(button, AxisDirection::Positive),
        );
        self
    }

    /// Adds a gamepad button binding connecting the given gamepad button to the given logical
    /// button.
    pub fn bind_gamepad_button_to_button(
        mut self,
        gamepad_button: GamepadButton,
        button: Buttons,
    ) -> Self {
        self.bindings.insert(
            InputType::GamepadButton(gamepad_button),
            InputEffect::Button(button, AxisDirection::Positive),
        );
        self
    }

    /// Adds a gamepad axis binding connecting the given gamepad axis to the given logical axis.
    pub fn bind_gamepad_axis_to_axis(
        mut self,
        gamepad_axis: GamepadAxis,
        axis: Axes,
        direction: AxisDirection,
    ) -> Self {
        self.bindings.insert(
            InputType::GamepadAxis(gamepad_axis),
            InputEffect::Axis(axis, direction),
        );
        self
    }

    /// Adds a mouse button binding connecting the given mouse button to the given logical button.
    pub fn bind_mouse_to_button(mut self, mouse_button: MouseButton, button: Buttons) -> Self {
        self.bindings.insert(
            InputType::MouseButton(mouse_button),
            InputEffect::Button(button, AxisDirection::Positive),
        );
        self
    }

    /// Adds a mouse scroll wheel binding connecting the given mouse scroll wheel axis to the given
    /// logical axis.
    pub fn bind_scroll_to_axis(
        mut self,
        scroll_axis: ScrollAxis,
        axis: Axes,
        direction: AxisDirection,
    ) -> Self {
        self.bindings.insert(
            InputType::MouseScroll(scroll_axis),
            InputEffect::Axis(axis, direction),
        );
        self
    }

    /// Takes a physical keycode input and turns it into a logical input type (keycode ->
    /// axis/button).
    pub fn resolve_keycode(
        &self,
        keycode: impl Into<Key>,
        state: bool,
    ) -> Option<InputEvent<Axes, Buttons>> {
        self.resolve_button_input(&InputType::KeyCode(keycode.into()), state)
    }

    pub fn resolve_scancode(
        &self,
        scancode: impl Into<Key>,
        state: bool,
    ) -> Option<InputEvent<Axes, Buttons>> {
        self.resolve_button_input(&InputType::ScanCode(scancode.into()), state)
    }

    fn resolve_button_input(
        &self,
        input_type: &InputType,
        state: bool,
    ) -> Option<InputEvent<Axes, Buttons>> {
        Some(
            self.bindings
                .get(input_type)?
                .clone()
                .to_event_with_button_state(state),
        )
    }

    /// Convert a physical gamepad input into a logical input.
    pub fn resolve_gamepad_button(
        &self,
        button: impl Into<GamepadButton>,
        state: bool,
    ) -> Option<InputEvent<Axes, Buttons>> {
        Some(
            self.bindings
                .get(&InputType::GamepadButton(button.into()))?
                .clone()
                .to_event_with_button_state(state),
        )
    }

    /// Convert a physical mouse button input into a logical input.
    pub fn resolve_mouse_button(
        &self,
        mouse_button: impl Into<MouseButton>,
        state: bool,
    ) -> Option<InputEvent<Axes, Buttons>> {
        Some(
            self.bindings
                .get(&InputType::MouseButton(mouse_button.into()))?
                .clone()
                .to_event_with_button_state(state),
        )
    }

    /// Convert a physical gamepad axis input into a logical input.
    pub fn resolve_gamepad_axis(
        &self,
        axis: impl Into<GamepadAxis>,
        position: f32,
    ) -> Option<InputEvent<Axes, Buttons>> {
        match self.bindings.get(&InputType::GamepadAxis(axis.into()))? {
            InputEffect::Axis(axis, direction) => {
                Some(InputEvent::Axis(axis.clone(), direction.sign() * position))
            }
            InputEffect::Button(button, direction) => Some(InputEvent::Button {
                button: button.clone(),
                state: (direction.sign() * position).is_sign_positive(),
                repeat: false,
            }),
        }
    }

    pub fn resolve_scroll_axis(
        &self,
        axis: impl Into<ScrollAxis>,
        position: f32,
    ) -> Option<InputEvent<Axes, Buttons>> {
        match self.bindings.get(&InputType::MouseScroll(axis.into()))? {
            InputEffect::Axis(axis, direction) => {
                Some(InputEvent::Axis(axis.clone(), direction.sign() * position))
            }
            InputEffect::Button(button, direction) => Some(InputEvent::Button {
                button: button.clone(),
                state: (direction.sign() * position).is_sign_positive(),
                repeat: false,
            }),
        }
    }

    /// Convert a generic input event into a logical input.
    pub fn resolve_generic_input_event(
        &self,
        event: InputEvent<GenericAxis, GenericButton>,
    ) -> Option<InputEvent<Axes, Buttons>> {
        match event {
            InputEvent::Axis(axis, position) => match axis {
                GenericAxis::Gamepad(gamepad_axis) => {
                    self.resolve_gamepad_axis(gamepad_axis, position)
                }
                GenericAxis::Mouse(scroll_axis) => self.resolve_scroll_axis(scroll_axis, position),
            },
            InputEvent::Button { button, state, .. } => match button {
                GenericButton::Gamepad(gamepad_button) => {
                    self.resolve_gamepad_button(gamepad_button, state)
                }
                GenericButton::KeyCode(keycode, _modifiers) => self.resolve_keycode(keycode, state),
                GenericButton::Mouse(mouse_button, _modifiers) => {
                    self.resolve_mouse_button(mouse_button, state)
                }
                GenericButton::ScanCode(scancode, _modifiers) => {
                    self.resolve_scancode(scancode, state)
                }
            },
            InputEvent::Cursor(position) => Some(InputEvent::Cursor(position)),
        }
    }
}

/// Represents an input state for a given set of logical axes and buttons, plus a cursor.
#[derive(Debug)]
pub struct InputState<Axes, Buttons>
where
    Axes: Hash + Eq + Clone,
    Buttons: Hash + Eq + Clone,
{
    // Input state for axes
    axes: HashMap<Axes, AxisState>,
    // Input states for buttons
    buttons: HashMap<Buttons, ButtonState>,
    // Input state for the mouse cursor
    mouse: CursorState,
}

impl<Axes, Buttons> Default for InputState<Axes, Buttons>
where
    Axes: Mappable,
    Buttons: Mappable,
{
    fn default() -> Self {
        Self::new()
    }
}

impl<Axes, Buttons> InputState<Axes, Buttons>
where
    Axes: Mappable,
    Buttons: Mappable,
{
    /// Create a fresh [`InputState`].
    pub fn new() -> Self {
        InputState {
            axes: HashMap::new(),
            buttons: HashMap::new(),
            mouse: CursorState::default(),
        }
    }

    /// Updates the logical input state based on the actual physical input state.  Should be called
    /// in your update() handler. So, it will do things like move the axes and so on.
    pub fn update(&mut self, dt: f32) {
        for (_axis, axis_status) in self.axes.iter_mut() {
            if axis_status.direction != 0.0 {
                // Accelerate the axis towards the input'ed direction.
                let vel = axis_status.acceleration * dt;
                let pending_position = axis_status.position
                    + if axis_status.direction > 0.0 {
                        vel
                    } else {
                        -vel
                    };
                axis_status.position = pending_position.clamp(-1., 1.);
            } else {
                // Gravitate back towards 0.
                let abs_dx = f32::min(axis_status.gravity * dt, f32::abs(axis_status.position));
                let dx = if axis_status.position > 0.0 {
                    -abs_dx
                } else {
                    abs_dx
                };
                axis_status.position += dx;
            }
        }

        for (_button, button_status) in self.buttons.iter_mut() {
            button_status.pressed_last_frame = button_status.pressed;
        }

        self.mouse.delta = self.mouse.position - self.mouse.last_position;
        self.mouse.last_position = self.mouse.position;
    }

    /// This method should get called by your key_down_event handler.
    pub fn update_button_down(&mut self, button: Buttons, repeat: bool) {
        self.update_event(InputEvent::Button {
            button,
            state: true,
            repeat,
        });
    }

    /// This method should get called by your key_up_event handler.
    pub fn update_button_up(&mut self, button: Buttons) {
        self.update_event(InputEvent::Button {
            button,
            state: false,
            repeat: false,
        });
    }

    /// This method should get called by your gamepad_axis_changed_event handler, or by your
    /// key_down_event handler if you're binding keypresses to logical axes.
    pub fn update_axis_start(&mut self, axis: Axes, position: f32) {
        self.update_event(InputEvent::Axis(axis, position));
    }

    /// This method will probably not usually be used; however, if you're connecting logical axes to
    /// physical button or key presses, then you can call this in your key_up_event handler for the
    /// corresponding button/key releases.
    pub fn update_axis_stop(&mut self, axis: Axes) {
        self.update_event(InputEvent::Axis(axis, 0.));
    }

    /// This method should be called by your mouse_motion_event handler.
    pub fn update_mouse_position(&mut self, position: Point2<f32>) {
        self.update_event(InputEvent::Cursor(position));
    }

    /// Takes an InputEffect and actually applies it.
    pub fn update_event(&mut self, event: InputEvent<Axes, Buttons>) {
        match event {
            InputEvent::Axis(axis, position) => {
                let f = AxisState::default;
                let axis_status = self.axes.entry(axis).or_insert_with(f);
                axis_status.direction = position;
            }
            InputEvent::Button {
                button,
                state,
                repeat,
            } => {
                let button_status = self.buttons.entry(button).or_default();
                button_status.pressed = state;
                button_status.repeat = repeat;
            }
            InputEvent::Cursor(position) => self.mouse.position = position,
        }
    }

    /// Get the position of a logical axis.
    pub fn get_axis(&self, axis: Axes) -> f32 {
        let d = AxisState::default();
        let axis_status = self.axes.get(&axis).unwrap_or(&d);
        axis_status.position
    }

    /// Get the *actual* position of a logical axis. We actually smooth axes a bit; you usually
    /// don't want this, but this method will return the actual exact position value of the axis.
    pub fn get_axis_raw(&self, axis: Axes) -> f32 {
        let d = AxisState::default();
        let axis_status = self.axes.get(&axis).unwrap_or(&d);
        axis_status.direction
    }

    fn get_button(&self, button: Buttons) -> ButtonState {
        let d = ButtonState::default();
        let button_status = self.buttons.get(&button).unwrap_or(&d);
        *button_status
    }

    /// Check if a logical button is down.
    pub fn get_button_down(&self, button: Buttons) -> bool {
        self.get_button(button).pressed
    }

    /// Check if a logical button is up.
    pub fn get_button_up(&self, button: Buttons) -> bool {
        !self.get_button(button).pressed
    }

    /// Returns whether or not the button was pressed this frame, only returning true if the press
    /// happened this frame.
    ///
    /// Basically, `get_button_down()` and `get_button_up()` are level triggers, this and
    /// `get_button_released()` are edge triggered.
    pub fn get_button_pressed(&self, button: Buttons) -> bool {
        let b = self.get_button(button);
        b.pressed && !b.pressed_last_frame
    }

    /// Check whether or not a button was released on this frame.
    pub fn get_button_released(&self, button: Buttons) -> bool {
        let b = self.get_button(button);
        !b.pressed && b.pressed_last_frame
    }

    /// Get the current mouse position.
    pub fn mouse_position(&self) -> Point2<f32> {
        self.mouse.position
    }

    /// Get the change in the mouse position for this frame with respect to the previous frame.
    pub fn mouse_delta(&self) -> Vector2<f32> {
        self.mouse.delta
    }

    /// Reset the input state, all axes at zero, all buttons unpresseed, all positions and deltas
    /// zeroed out.
    pub fn reset_input_state(&mut self) {
        for (_axis, axis_status) in self.axes.iter_mut() {
            axis_status.position = 0.0;
            axis_status.direction = 0.0;
        }

        for (_button, button_status) in self.buttons.iter_mut() {
            button_status.pressed = false;
            button_status.pressed_last_frame = false;
            button_status.repeat = false;
        }

        self.mouse.position = Point2::origin();
        self.mouse.last_position = Point2::origin();
        self.mouse.delta = Vector2::zeros();
    }
}

pub type GenericInputState = InputState<GenericAxis, GenericButton>;

/// Possible events coming in from whatever is hosting this program. Normally this will be a window
/// manager - hence `WindowEvent`, and which is why this enum contains so many window-centric types.
/// On some platforms (like a console) we expect many of these not to be fired.
#[derive(Debug, Clone)]
pub enum WindowEvent<Axes, Buttons>
where
    Axes: Mappable,
    Buttons: Mappable,
{
    /// An input event mapped to the parameterized axis/button types.
    Mapped(InputEvent<Axes, Buttons>),
    /// An input event corresponding to a user's attempt to input text. Unlike a character input
    /// this is processed w/ whatever modifiers, input adapters, etc. are external to the program.
    Text(char),
    /// Notification of a change in content scale (for example a window being moved from a low-DPI
    /// to a high-DPI monitor, or vice versa) if applicable.
    ContentScale(Vector2<f32>),
    /// Notification of a change in the backbuffer size.
    FramebufferSize(Vector2<u32>),
    /// Notification of the cursor entering (true) or leaving (false) the window, if applicable.
    WindowCursorEnter(bool),
    /// Notification of acquisition or loss of focus.
    WindowFocus(bool),
    /// Notification of a change in the position of the window, if applicable.
    WindowPos(Point2<u32>),
    /// Notification of a change in the size of the window, if applicable.
    WindowSize(Vector2<u32>),
    /// Notification of minimization (being "iconified.")
    WindowMinimize(bool),
    /// Notification of maximization.
    WindowMaximize(bool),
    /// Notification that the window contents have been "damaged" and need to be refreshed.
    WindowRefresh,
    /// Notification that the window has been requested to be closed.
    WindowClose,
    /// Notification that the window has had one or more files dropped into it.
    WindowFileDrop(Vec<PathBuf>),
}

pub type GenericWindowEvent = WindowEvent<GenericAxis, GenericButton>;

impl<Axes, Buttons> UserData for InputEvent<Axes, Buttons>
where
    Axes: LuaMappable + 'static,
    Buttons: LuaMappable + 'static,
{
}

impl<Axes, Buttons> UserData for InputState<Axes, Buttons>
where
    Axes: LuaMappable + 'static,
    Buttons: LuaMappable + 'static,
{
    #[allow(clippy::unit_arg)]
    fn add_methods<'lua, M: UserDataMethods<'lua, Self>>(methods: &mut M) {
        methods.add_method_mut("update_event", |_, this, ev| Ok(this.update_event(ev)));
        methods.add_method_mut("update", |_, this, dt| Ok(this.update(dt)));
        methods.add_method("get_axis", |_, this, axis| Ok(this.get_axis(axis)));
        methods.add_method("get_axis_raw", |_, this, axis| Ok(this.get_axis_raw(axis)));
        methods.add_method("get_button_down", |_, this, b| Ok(this.get_button_down(b)));
        methods.add_method("get_button_up", |_, this, b| Ok(this.get_button_up(b)));
        methods.add_method("get_button_pressed", |_, t, b| Ok(t.get_button_pressed(b)));
        methods.add_method(
            "get_button_released",
            |_, t, b| Ok(t.get_button_released(b)),
        );
        methods.add_method("mouse_position", |_, t, ()| Ok(t.mouse_position()));
        methods.add_method("mouse_delta", |_, t, ()| Ok(t.mouse_delta()));
        methods.add_method_mut("reset_input_state", |_, t, ()| Ok(t.reset_input_state()));
    }

    fn add_type_methods<'lua, M: UserDataMethods<'lua, Type<Self>>>(methods: &mut M) {
        methods.add_function("new", |_, ()| Ok(Self::new()));
    }
}
