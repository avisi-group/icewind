use {
    alloc::{borrow::ToOwned, string::ToString},
    core::fmt::{self, Display, Formatter},
};

#[derive(Debug, Clone, Copy)]
pub struct Range {
    start: usize,
    end: Option<usize>,
}

impl Display for Range {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{}..{}",
            self.start,
            self.end
                .as_ref()
                .map(ToString::to_string)
                .unwrap_or("".to_owned())
        )
    }
}

impl Range {
    pub fn new(start: usize, end: usize) -> Self {
        Self {
            start,
            end: Some(end),
        }
    }

    pub fn new_partial(start: usize) -> Self {
        Self { start, end: None }
    }

    pub fn start(&self) -> usize {
        self.start
    }

    pub fn end(&self) -> Option<usize> {
        self.end
    }

    pub fn end_mut(&mut self) -> &mut Option<usize> {
        &mut self.end
    }

    pub fn is_partial(&self) -> bool {
        self.end().is_none()
    }

    pub fn intersects(&self, other: &Self) -> bool {
        let Some(self_end) = self.end() else {
            panic!("`self` Range must be complete")
        };
        let Some(other_end) = other.end() else {
            panic!("`other` Range must be complete")
        };

        self.start() <= other_end && other.start() <= self_end
    }

    // start-inclusive, end-exclusive
    pub fn contains(&self, index: usize) -> bool {
        let Some(self_end) = self.end() else {
            panic!("`self` Range must be complete")
        };

        (self.start() <= index) && (index < self_end)
    }
}
