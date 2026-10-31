use crate::document::Delta;
use ratatui::layout::Rect;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct View {
    pub document: usize,
    pub cursor: usize,
    pub anchor: Option<usize>,
    pub top: usize,
    pub wrap_top: usize,
    pub left: usize,
    #[serde(skip)]
    pub preferred_column: Option<usize>,
}

impl View {
    pub fn selection(&self) -> std::ops::Range<usize> {
        let anchor = self.anchor.unwrap_or(self.cursor);
        anchor.min(self.cursor)..anchor.max(self.cursor)
    }

    pub fn apply(&mut self, delta: Delta) {
        self.cursor = delta.transform(self.cursor);
        self.anchor = self.anchor.map(|a| delta.transform(a));
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum Pane {
    Leaf(usize),
    Split {
        vertical: bool,
        first: Box<Pane>,
        second: Box<Pane>,
    },
}

impl Default for Pane {
    fn default() -> Self {
        Self::Leaf(0)
    }
}

impl Pane {
    pub fn split(&mut self, target: usize, next: usize, vertical: bool) -> bool {
        match self {
            Self::Leaf(id) if *id == target => {
                *self = Self::Split {
                    vertical,
                    first: Box::new(Self::Leaf(target)),
                    second: Box::new(Self::Leaf(next)),
                };
                true
            }
            Self::Leaf(_) => false,
            Self::Split { first, second, .. } => {
                first.split(target, next, vertical) || second.split(target, next, vertical)
            }
        }
    }

    pub fn remove(&mut self, target: usize) -> bool {
        match self {
            Self::Leaf(_) => false,
            Self::Split { first, second, .. } => {
                if matches!(first.as_ref(), Self::Leaf(id) if *id == target) {
                    *self = *second.clone();
                    true
                } else if matches!(second.as_ref(), Self::Leaf(id) if *id == target) {
                    *self = *first.clone();
                    true
                } else {
                    first.remove(target) || second.remove(target)
                }
            }
        }
    }

    pub fn rectangles(&self, area: Rect, output: &mut Vec<(usize, Rect)>) {
        match self {
            Self::Leaf(id) => output.push((*id, area)),
            Self::Split {
                vertical,
                first,
                second,
            } => {
                let mut a = area;
                let mut b = area;
                if *vertical {
                    a.width = area.width / 2;
                    b.x = b.x.saturating_add(a.width);
                    b.width = area.width.saturating_sub(a.width);
                } else {
                    a.height = area.height / 2;
                    b.y = b.y.saturating_add(a.height);
                    b.height = area.height.saturating_sub(a.height);
                }
                first.rectangles(a, output);
                second.rectangles(b, output);
            }
        }
    }

    pub fn leaves(&self) -> Vec<usize> {
        let mut rectangles = Vec::new();
        self.rectangles(Rect::default(), &mut rectangles);
        rectangles.into_iter().map(|(id, _)| id).collect()
    }
}
