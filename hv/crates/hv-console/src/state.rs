use std::sync::Arc;

use egui::mutex::Mutex;

use egui::*;

use super::{CCursorRange, CursorRange};

type Undoer = egui::util::undoer::Undoer<(Section<CCursorRange>, String)>;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Deserialize, serde::Serialize))]
pub enum Section<T> {
    History(T),
    Buffer(T),
}

impl<T: Default> Default for Section<T> {
    fn default() -> Self {
        Self::Buffer(T::default())
    }
}

impl<T> Section<T> {
    pub fn map<U>(self, f: impl FnOnce(T) -> U) -> Section<U> {
        match self {
            Self::History(t) => Section::History(f(t)),
            Self::Buffer(t) => Section::Buffer(f(t)),
        }
    }

    pub fn map_select<U, V>(self, history: V, buffer: V, f: impl FnOnce(V, T) -> U) -> Section<U> {
        match self {
            Self::History(t) => Section::History(f(history, t)),
            Self::Buffer(t) => Section::Buffer(f(buffer, t)),
        }
    }

    pub fn either(&self) -> &T {
        match self {
            Self::History(t) => t,
            Self::Buffer(t) => t,
        }
    }
}

impl Section<CursorRange> {
    pub fn as_ccursor_range(&self) -> Section<CCursorRange> {
        self.map(|cr| cr.as_ccursor_range())
    }

    pub fn is_empty(&self) -> bool {
        self.either().is_empty()
    }
}

/// The text edit state stored between frames.
#[derive(Clone, Default)]
#[cfg_attr(feature = "serde", derive(serde::Deserialize, serde::Serialize))]
#[cfg_attr(feature = "serde", serde(default))]
pub struct ConsoleState {
    cursor_range: Option<Section<CursorRange>>,

    /// This is what is easiest to work with when editing text,
    /// so users are more likely to read/write this.
    ccursor_range: Option<Section<CCursorRange>>,

    /// Wrapped in Arc for cheaper clones.
    #[cfg_attr(feature = "serde", serde(skip))]
    pub(crate) undoer: Arc<Mutex<Undoer>>,

    // If IME candidate window is shown on this text edit.
    #[cfg_attr(feature = "serde", serde(skip))]
    pub(crate) has_ime: bool,

    // Visual offset when editing singleline text bigger than the width.
    #[cfg_attr(feature = "serde", serde(skip))]
    pub(crate) singleline_offset: f32,
}

impl ConsoleState {
    pub fn load(ctx: &Context, id: Id) -> Option<Self> {
        ctx.memory().data.get_persisted(id)
    }

    pub fn store(self, ctx: &Context, id: Id) {
        ctx.memory().data.insert_persisted(id, self);
    }

    /// The the currently selected range of characters.
    pub fn ccursor_range(&self) -> Option<Section<CCursorRange>> {
        self.ccursor_range.or_else(|| {
            self.cursor_range
                .map(|cursor_range| cursor_range.as_ccursor_range())
        })
    }

    /// Sets the currently selected range of characters.
    pub fn set_ccursor_range(&mut self, ccursor_range: Option<Section<CCursorRange>>) {
        self.cursor_range = None;
        self.ccursor_range = ccursor_range;
    }

    pub fn set_cursor_range(&mut self, cursor_range: Option<Section<CursorRange>>) {
        self.cursor_range = cursor_range;
        self.ccursor_range = None;
    }

    pub fn cursor_range(
        &mut self,
        history_galley: &Galley,
        buffer_galley: &Galley,
    ) -> Option<Section<CursorRange>> {
        self.cursor_range
            .map(|section| {
                // We only use the PCursor (paragraph number, and character offset within that paragraph).
                // This is so that if we resize the `TextEdit` region, and text wrapping changes,
                // we keep the same byte character offset from the beginning of the text,
                // even though the number of rows changes
                // (each paragraph can be several rows, due to word wrapping).
                // The column (character offset) should be able to extend beyond the last word so that we can
                // go down and still end up on the same column when we return.
                match section {
                    Section::History(cursor_range) => Section::History(CursorRange {
                        primary: history_galley.from_pcursor(cursor_range.primary.pcursor),
                        secondary: history_galley.from_pcursor(cursor_range.secondary.pcursor),
                    }),
                    Section::Buffer(cursor_range) => Section::Buffer(CursorRange {
                        primary: buffer_galley.from_pcursor(cursor_range.primary.pcursor),
                        secondary: buffer_galley.from_pcursor(cursor_range.secondary.pcursor),
                    }),
                }
            })
            .or_else(|| {
                self.ccursor_range.map(|section| match section {
                    Section::History(ccursor_range) => Section::History(CursorRange {
                        primary: history_galley.from_ccursor(ccursor_range.primary),
                        secondary: history_galley.from_ccursor(ccursor_range.secondary),
                    }),
                    Section::Buffer(ccursor_range) => Section::Buffer(CursorRange {
                        primary: buffer_galley.from_ccursor(ccursor_range.primary),
                        secondary: buffer_galley.from_ccursor(ccursor_range.secondary),
                    }),
                })
            })
    }
}
