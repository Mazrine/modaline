//! Pixel operations: an authored fix as data instead of as strokes.
//!
//! A fix can be recorded as a list of `Op`s and replayed through
//! [`Editor::apply`]. Every write goes through `paint_pixel` / `record`, so a
//! replay is one undo step and produces exactly the pixels a hand fix would.
//! The list is `serde`-friendly: another tool, a test, or an agent can hand one over.

use serde::{Deserialize, Serialize};

use crate::color::Rgba;
use crate::editor::{Editor, UndoStep};
use crate::sprite::{
    BlendMode, FillOptions, blend_over, ellipse_points, flood_fill_tol, line_points,
    rect_filled_points, rect_points,
};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum Op {
    Px {
        x: u32,
        y: u32,
        color: Rgba,
    },
    Line {
        x0: i32,
        y0: i32,
        x1: i32,
        y1: i32,
        color: Rgba,
    },
    Rect {
        x0: i32,
        y0: i32,
        x1: i32,
        y1: i32,
        #[serde(default)]
        filled: bool,
        color: Rgba,
    },
    Ellipse {
        x0: i32,
        y0: i32,
        x1: i32,
        y1: i32,
        #[serde(default)]
        filled: bool,
        color: Rgba,
    },
    /// Contiguous fill of the colour under `(x, y)`.
    Fill {
        x: u32,
        y: u32,
        color: Rgba,
    },
    /// Alpha-over the pixel that is already there, instead of replacing it.
    Blend {
        x: u32,
        y: u32,
        color: Rgba,
    },
    PaletteLock {
        on: bool,
    },
}

impl Editor {
    /// Apply an op list as one undo step. Returns the number of pixels changed.
    pub fn apply(&mut self, ops: &[Op]) -> usize {
        self.begin_stroke();
        for op in ops {
            self.apply_one(op);
        }
        self.end_stroke();
        match self.undo.last() {
            Some(UndoStep::Cells { cells }) => cells.len(),
            _ => 0,
        }
    }

    fn apply_one(&mut self, op: &Op) {
        match op {
            Op::Px { x, y, color } => self.paint_if_inside(*x, *y, *color),
            Op::Line {
                x0,
                y0,
                x1,
                y1,
                color,
            } => self.paint_points(line_points(*x0, *y0, *x1, *y1), *color),
            Op::Rect {
                x0,
                y0,
                x1,
                y1,
                filled,
                color,
            } => {
                let points = if *filled {
                    rect_filled_points(*x0, *y0, *x1, *y1)
                } else {
                    rect_points(*x0, *y0, *x1, *y1)
                };
                self.paint_points(points, *color);
            }
            Op::Ellipse {
                x0,
                y0,
                x1,
                y1,
                filled,
                color,
            } => self.paint_points(ellipse_points(*x0, *y0, *x1, *y1, *filled), *color),
            Op::Fill { x, y, color } => self.fill_at(*x, *y, *color),
            Op::Blend { x, y, color } => {
                let (l, f) = (self.active_layer, self.active_frame);
                if *x < self.sprite.width && *y < self.sprite.height {
                    let under = self.sprite.get(l, f, *x, *y);
                    let over = blend_over(BlendMode::Normal, *color, color.3 as u32, under);
                    self.paint_pixel(*x, *y, over);
                }
            }
            Op::PaletteLock { on } => self.set_palette_lock(*on),
        }
    }

    fn paint_points(&mut self, points: Vec<(u32, u32)>, color: Rgba) {
        for (x, y) in points {
            self.paint_if_inside(x, y, color);
        }
    }

    /// The point helpers take `i32` and cast blind, so a line that starts off
    /// canvas arrives here as a huge `u32`: clip instead of panicking.
    fn paint_if_inside(&mut self, x: u32, y: u32, color: Rgba) {
        if x < self.sprite.width && y < self.sprite.height {
            self.paint_pixel(x, y, color);
        }
    }

    fn fill_at(&mut self, x: u32, y: u32, color: Rgba) {
        let (l, f) = (self.active_layer, self.active_frame);
        if x >= self.sprite.width || y >= self.sprite.height {
            return;
        }
        if !self.sprite.layer_editable(l) {
            self.msg = "layer is locked or hidden".into();
            return;
        }
        if self.palette_lock && !color.is_transparent() && !self.palette.contains(&color) {
            self.msg = "palette locked: that colour is not in it".into();
            return;
        }
        let clip = self.has_selection().then(|| self.selection_grid());
        let opts = FillOptions {
            tol: self.wand_tolerance,
            contiguous: self.contiguous,
            clip: clip.as_deref(),
        };
        let changes = flood_fill_tol(&mut self.sprite, l, f, x, y, color, opts);
        for (cx, cy, old, new) in &changes {
            self.record(l, f, *cx, *cy, *old, *new);
        }
        if !changes.is_empty() {
            self.dirty = true;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn editor() -> Editor {
        Editor::new(8, 8)
    }

    fn bytes(e: &Editor) -> Vec<Rgba> {
        e.sprite.composite(0)
    }

    fn sample_ops() -> Vec<Op> {
        vec![
            Op::Px {
                x: 1,
                y: 1,
                color: Rgba::rgb(200, 30, 40),
            },
            Op::Line {
                x0: 0,
                y0: 4,
                x1: 5,
                y1: 4,
                color: Rgba::rgb(10, 20, 30),
            },
            Op::Rect {
                x0: 2,
                y0: 2,
                x1: 4,
                y1: 3,
                filled: true,
                color: Rgba::rgb(90, 200, 120),
            },
            Op::Ellipse {
                x0: 5,
                y0: 5,
                x1: 7,
                y1: 7,
                filled: false,
                color: Rgba::rgb(240, 240, 240),
            },
            Op::Fill {
                x: 0,
                y: 0,
                color: Rgba::rgb(0, 0, 0),
            },
            Op::Blend {
                x: 1,
                y: 1,
                color: Rgba(255, 255, 255, 128),
            },
        ]
    }

    #[test]
    fn ops_are_clipped_to_the_selection() {
        let mut e = editor();
        e.selection = Some(crate::editor::Selection::rect(2, 2, 5, 5));
        e.apply(&[
            Op::Px {
                x: 0,
                y: 0,
                color: Rgba::rgb(200, 0, 0),
            },
            Op::Px {
                x: 3,
                y: 3,
                color: Rgba::rgb(0, 200, 0),
            },
        ]);
        assert!(e.sprite.get(0, 0, 0, 0).is_transparent());
        assert_eq!(e.sprite.get(0, 0, 3, 3), Rgba::rgb(0, 200, 0));

        let mut f = editor();
        for x in 0..8 {
            for y in 0..8 {
                f.sprite.set(0, 0, x, y, Rgba::rgb(10, 10, 10));
            }
        }
        f.selection = Some(crate::editor::Selection::rect(2, 2, 4, 4));
        f.apply(&[Op::Fill {
            x: 2,
            y: 2,
            color: Rgba::rgb(0, 0, 200),
        }]);
        assert_eq!(f.sprite.get(0, 0, 3, 3), Rgba::rgb(0, 0, 200));
        assert_eq!(f.sprite.get(0, 0, 0, 0), Rgba::rgb(10, 10, 10));
    }

    #[test]
    fn a_recorded_fix_replays_byte_identically() {
        let mut first = editor();
        let painted = first.apply(&sample_ops());
        assert!(painted > 40, "the sample should touch most of the canvas");

        // The list travels as JSON, not as pixels.
        let json = serde_json::to_string(&sample_ops()).unwrap();
        let replayed_ops: Vec<Op> = serde_json::from_str(&json).unwrap();
        let mut replayed = editor();
        replayed.apply(&replayed_ops);

        assert_eq!(bytes(&first), bytes(&replayed));
    }

    #[test]
    fn an_op_list_is_one_undo_step() {
        let mut e = editor();
        let before = bytes(&e);
        e.apply(&sample_ops());
        assert_ne!(bytes(&e), before);
        e.undo();
        assert_eq!(bytes(&e), before);
    }

    #[test]
    fn ops_that_start_off_canvas_are_clipped_not_panicked() {
        let mut e = editor();
        let painted = e.apply(&[
            Op::Line {
                x0: -4,
                y0: 2,
                x1: 3,
                y1: 2,
                color: Rgba::rgb(1, 2, 3),
            },
            Op::Rect {
                x0: 6,
                y0: 6,
                x1: 40,
                y1: 40,
                filled: true,
                color: Rgba::rgb(4, 5, 6),
            },
            Op::Px {
                x: 99,
                y: 99,
                color: Rgba::rgb(7, 8, 9),
            },
        ]);
        assert_eq!(painted, 4 + 4);
    }

    #[test]
    fn a_locked_palette_refuses_an_op_colour() {
        let mut e = editor();
        let allowed = Rgba::rgb(10, 120, 200);
        e.load_palette(vec![allowed]);
        e.set_palette_lock(true);
        e.apply(&[
            Op::Px {
                x: 2,
                y: 2,
                color: Rgba::rgb(1, 1, 1),
            },
            Op::Px {
                x: 3,
                y: 3,
                color: allowed,
            },
        ]);
        let comp = bytes(&e);
        assert!(comp[2 * 8 + 2].is_transparent());
        assert_eq!(comp[3 * 8 + 3], allowed);
        assert!(e.msg.contains("palette locked"));
    }

    #[test]
    fn blend_lays_the_colour_over_what_is_there() {
        let mut e = editor();
        e.apply(&[Op::Px {
            x: 0,
            y: 0,
            color: Rgba::rgb(0, 0, 0),
        }]);
        e.apply(&[Op::Blend {
            x: 0,
            y: 0,
            color: Rgba(255, 255, 255, 128),
        }]);
        let px = bytes(&e)[0];
        assert!(px.0 > 120 && px.0 < 136, "half white over black: {px:?}");
        assert_eq!(px.3, 255);
    }
}
