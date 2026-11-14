//! Toolbox: tool groups with flyouts, the same grouping as the reference app.

use egui::{Color32, CornerRadius, Popup, PopupCloseBehavior, PopupKind, RectAlign, Sense, SetOpenCommand, Stroke, Vec2};
use egui_material_icons::{MaterialIcon, icons::*};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Tool {
    Pick, FreehandPick, FreeTransform,
    Shape, Smooth, Smudge, Twirl, AttractRepel, SmearBrush, RoughenBrush,
    Crop, Knife, VirtualSegmentDelete, Eraser,
    Zoom, Pan,
    Freehand, TwoPointLine, Bezier, Pen, BSpline, Polyline, ThreePointCurve,
    PaintBrush, ArtisticMedia, Sketch, SmartDrawing,
    Rectangle, ThreePointRectangle,
    Ellipse, ThreePointEllipse,
    Polygon, Star, Spiral, CommonShapes, Impact, GraphPaper,
    Text, Table,
    ParallelDimension, HorizontalVerticalDimension, AngularDimension, SegmentDimension, Callout,
    Connector, EditAnchor,
    DropShadow, Contour, Blend, Distort, Envelope, Extrude, BlockShadow,
    Transparency,
    ColorEyedropper, AttributesEyedropper,
    InteractiveFill, SmartFill, MeshFill,
    VariableOutline,
}

pub struct ToolGroup {
    pub tools: &'static [Tool],
}

pub const GROUPS: &[ToolGroup] = &[
    ToolGroup { tools: &[Tool::Pick, Tool::FreehandPick, Tool::FreeTransform] },
    ToolGroup { tools: &[Tool::Shape, Tool::Smooth, Tool::Smudge, Tool::Twirl, Tool::AttractRepel, Tool::SmearBrush, Tool::RoughenBrush] },
    ToolGroup { tools: &[Tool::Crop, Tool::Knife, Tool::VirtualSegmentDelete, Tool::Eraser] },
    ToolGroup { tools: &[Tool::Zoom, Tool::Pan] },
    ToolGroup { tools: &[Tool::Freehand, Tool::TwoPointLine, Tool::Bezier, Tool::Pen, Tool::BSpline, Tool::Polyline, Tool::ThreePointCurve] },
    ToolGroup { tools: &[Tool::PaintBrush, Tool::ArtisticMedia, Tool::Sketch, Tool::SmartDrawing] },
    ToolGroup { tools: &[Tool::Rectangle, Tool::ThreePointRectangle] },
    ToolGroup { tools: &[Tool::Ellipse, Tool::ThreePointEllipse] },
    ToolGroup { tools: &[Tool::Polygon, Tool::Star, Tool::Spiral, Tool::CommonShapes, Tool::Impact, Tool::GraphPaper] },
    ToolGroup { tools: &[Tool::Text, Tool::Table] },
    ToolGroup { tools: &[Tool::ParallelDimension, Tool::HorizontalVerticalDimension, Tool::AngularDimension, Tool::SegmentDimension, Tool::Callout] },
    ToolGroup { tools: &[Tool::Connector, Tool::EditAnchor] },
    ToolGroup { tools: &[Tool::DropShadow, Tool::Contour, Tool::Blend, Tool::Distort, Tool::Envelope, Tool::Extrude, Tool::BlockShadow] },
    ToolGroup { tools: &[Tool::Transparency] },
    ToolGroup { tools: &[Tool::ColorEyedropper, Tool::AttributesEyedropper] },
    ToolGroup { tools: &[Tool::InteractiveFill, Tool::SmartFill, Tool::MeshFill] },
    ToolGroup { tools: &[Tool::VariableOutline] },
];

impl Tool {
    pub fn label(self) -> &'static str {
        use Tool::*;
        match self {
            Pick => "Pick", FreehandPick => "Freehand Pick", FreeTransform => "Free Transform",
            Shape => "Shape", Smooth => "Smooth", Smudge => "Smudge", Twirl => "Twirl",
            AttractRepel => "Attract and Repel", SmearBrush => "Smear", RoughenBrush => "Roughen",
            Crop => "Crop", Knife => "Knife", VirtualSegmentDelete => "Virtual Segment Delete", Eraser => "Eraser",
            Zoom => "Zoom", Pan => "Pan",
            Freehand => "Freehand", TwoPointLine => "2-Point Line", Bezier => "Bézier", Pen => "Pen",
            BSpline => "B-Spline", Polyline => "Polyline", ThreePointCurve => "3-Point Curve",
            PaintBrush => "Paint Brush", ArtisticMedia => "Artistic Media", Sketch => "Sketch", SmartDrawing => "Smart Drawing",
            Rectangle => "Rectangle", ThreePointRectangle => "3-Point Rectangle",
            Ellipse => "Ellipse", ThreePointEllipse => "3-Point Ellipse",
            Polygon => "Polygon", Star => "Star", Spiral => "Spiral", CommonShapes => "Common Shapes", Impact => "Impact", GraphPaper => "Graph Paper",
            Text => "Text", Table => "Table",
            ParallelDimension => "Parallel Dimension", HorizontalVerticalDimension => "Horizontal or Vertical Dimension",
            AngularDimension => "Angular Dimension", SegmentDimension => "Segment Dimension", Callout => "3-Point Callout",
            Connector => "Connector", EditAnchor => "Edit Anchor",
            DropShadow => "Drop Shadow", Contour => "Contour", Blend => "Blend", Distort => "Distort",
            Envelope => "Envelope", Extrude => "Extrude", BlockShadow => "Block Shadow",
            Transparency => "Transparency",
            ColorEyedropper => "Color Eyedropper", AttributesEyedropper => "Attributes Eyedropper",
            InteractiveFill => "Interactive Fill", SmartFill => "Smart Fill", MeshFill => "Mesh Fill",
            VariableOutline => "Variable Outline",
        }
    }

    pub fn shortcut(self) -> Option<&'static str> {
        use Tool::*;
        Some(match self {
            Shape => "F10",
            Zoom => "Z",
            Pan => "H",
            Freehand => "F5",
            ArtisticMedia => "I",
            Rectangle => "F6",
            Ellipse => "F7",
            Polygon => "Y",
            Spiral => "A",
            GraphPaper => "D",
            Text => "F8",
            Eraser => "X",
            InteractiveFill => "G",
            MeshFill => "M",
            _ => return None,
        })
    }

    pub fn icon(self) -> MaterialIcon {
        use Tool::*;
        match self {
            Pick => ICON_NEAR_ME,
            FreehandPick => ICON_GESTURE,
            FreeTransform => ICON_TRANSFORM,
            Shape => ICON_SHAPE_LINE,
            Smooth | Smudge | Twirl | AttractRepel | SmearBrush | RoughenBrush => ICON_CYCLONE,
            Crop => ICON_CROP,
            Knife => ICON_CONTENT_CUT,
            VirtualSegmentDelete | Eraser => ICON_INK_ERASER,
            Zoom => ICON_ZOOM_IN,
            Pan => ICON_PAN_TOOL,
            Freehand => ICON_GESTURE,
            TwoPointLine => ICON_STRAIGHTEN,
            Bezier | Pen | ThreePointCurve => ICON_INK_PEN,
            BSpline | Polyline => ICON_POLYLINE,
            PaintBrush | ArtisticMedia => ICON_BRUSH,
            Sketch | SmartDrawing => ICON_DRAW,
            Rectangle | ThreePointRectangle => ICON_RECTANGLE,
            Ellipse | ThreePointEllipse => ICON_CIRCLE,
            Polygon => ICON_PENTAGON,
            Star => ICON_STAR,
            Spiral => ICON_CYCLONE,
            CommonShapes => ICON_INTERESTS,
            Impact => ICON_AUTO_AWESOME,
            GraphPaper => ICON_GRID_ON,
            Text => ICON_TITLE,
            Table => ICON_TABLE,
            ParallelDimension | HorizontalVerticalDimension | AngularDimension | SegmentDimension
            | Callout => ICON_SQUARE_FOOT,
            Connector | EditAnchor => ICON_CONVERSION_PATH,
            DropShadow | BlockShadow => ICON_SHADOW,
            Contour => ICON_FILTER_CENTER_FOCUS,
            Blend => ICON_ANIMATION,
            Distort => ICON_CYCLONE,
            Envelope => ICON_FORMAT_SHAPES,
            Extrude => ICON_DEPLOYED_CODE,
            Transparency => ICON_OPACITY,
            ColorEyedropper | AttributesEyedropper => ICON_COLORIZE,
            InteractiveFill => ICON_GRADIENT,
            SmartFill => ICON_FORMAT_COLOR_FILL,
            MeshFill => ICON_GRID_ON,
            VariableOutline => ICON_LINE_WEIGHT,
        }
    }

    /// Whether the tool does something on the canvas in this build.
    pub fn is_live(self) -> bool {
        crate::tools::is_implemented(self)
    }

    pub fn group_index(self) -> usize {
        GROUPS
            .iter()
            .position(|g| g.tools.contains(&self))
            .expect("every tool belongs to a group")
    }
}

/// Which tool each group shows on its button (the last one used from the flyout).
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ToolboxState {
    shown: Vec<Tool>,
}

impl Default for ToolboxState {
    fn default() -> Self {
        Self { shown: GROUPS.iter().map(|g| g.tools[0]).collect() }
    }
}

impl ToolboxState {
    pub fn remember(&mut self, tool: Tool) {
        if self.shown.len() != GROUPS.len() {
            *self = Self::default();
        }
        self.shown[tool.group_index()] = tool;
    }
}

const BUTTON: egui::Vec2 = egui::vec2(crate::theme::TOOLBOX_WIDTH, 31.0);

/// Groups followed by a separator line.
const SEPARATOR_AFTER: &[usize] = &[1, 3, 8, 9, 11, 13, 15];

/// Draws the toolbox. Returns the tool the user picked, if any.
pub fn toolbox_ui(ui: &mut egui::Ui, state: &mut ToolboxState, active: Tool) -> Option<Tool> {
    use crate::theme;
    if state.shown.len() != GROUPS.len() {
        *state = ToolboxState::default();
    }
    let mut picked = None;
    ui.spacing_mut().item_spacing = Vec2::ZERO;
    for (i, group) in GROUPS.iter().enumerate() {
        let tool = state.shown[i];
        let has_flyout = group.tools.len() > 1;
        let selected = group.tools.contains(&active);

        let (rect, response) = ui.allocate_exact_size(BUTTON, Sense::click());
        let cell = egui::Rect::from_center_size(rect.center(), Vec2::splat(28.0));
        if selected {
            ui.painter().rect(cell, CornerRadius::same(5), theme::SUNKEN, Stroke::new(1.0, Color32::from_gray(0x10)), egui::StrokeKind::Inside);
        } else if response.hovered() {
            ui.painter().rect_filled(cell, CornerRadius::same(5), Color32::from_gray(0x2C));
        }
        let color = if !tool.is_live() {
            theme::TEXT_DISABLED
        } else if selected || response.hovered() {
            Color32::WHITE
        } else {
            theme::TEXT
        };
        ui.painter().text(
            rect.center(),
            egui::Align2::CENTER_CENTER,
            <&str>::from(tool.icon()),
            egui::FontId::new(19.0, tool.icon().font_family()),
            color,
        );
        if has_flyout {
            // Small corner triangle marks a flyout.
            let c = cell.right_bottom() + Vec2::new(1.0, 1.0);
            ui.painter().add(egui::Shape::convex_polygon(
                vec![c, c - Vec2::new(4.0, 0.0), c - Vec2::new(0.0, 4.0)],
                theme::TEXT,
                Stroke::NONE,
            ));
        }
        if SEPARATOR_AFTER.contains(&i) {
            let (sep, _) = ui.allocate_exact_size(egui::vec2(BUTTON.x, 9.0), Sense::hover());
            ui.painter().hline(sep.x_range().shrink(3.0), sep.center().y, Stroke::new(1.0, theme::SEPARATOR));
        }

        let tip = match tool.shortcut() {
            Some(key) => format!("{} ({key})", tool.label()),
            None => tool.label().to_owned(),
        };
        let tip = if tool.is_live() { tip } else { format!("{tip}\nNot available yet") };
        let response = response.on_hover_text(tip);

        let corner_hit = response
            .interact_pointer_pos()
            .is_some_and(|p| p.x > rect.right() - 12.0 && p.y > rect.bottom() - 12.0);
        let open_flyout = has_flyout
            && (response.secondary_clicked()
                || (response.clicked() && corner_hit)
                || response.long_touched());
        if response.clicked() && !open_flyout {
            picked = Some(tool);
        }

        if has_flyout {
            Popup::from_response(&response)
                .id(ui.scope_id().with(("tool_flyout", i)))
                .kind(PopupKind::Menu)
                .open_memory(open_flyout.then_some(SetOpenCommand::Bool(true)))
                .align(RectAlign::RIGHT_START)
                .close_behavior(PopupCloseBehavior::CloseOnClick)
                .show(|ui| {
                    ui.set_min_width(200.0);
                    for &t in group.tools {
                        let mut button = egui::Button::new(icon_label(t))
                            .selected(t == active)
                            .frame_when_inactive(false);
                        if let Some(key) = t.shortcut() {
                            button = button.right_text(key);
                        }
                        if ui.add(button).clicked() {
                            picked = Some(t);
                        }
                    }
                });
        }
    }
    if let Some(tool) = picked {
        state.remember(tool);
    }
    picked
}

fn icon_label(tool: Tool) -> egui::text::LayoutJob {
    let mut job = egui::text::LayoutJob::default();
    let icon = tool.icon();
    job.append(
        <&str>::from(icon),
        0.0,
        egui::TextFormat { font_id: egui::FontId::new(16.0, icon.font_family()), color: Color32::PLACEHOLDER, valign: egui::Align::Center, ..Default::default() },
    );
    job.append(
        tool.label(),
        8.0,
        egui::TextFormat { font_id: egui::FontId::proportional(13.0), color: Color32::PLACEHOLDER, valign: egui::Align::Center, ..Default::default() },
    );
    job
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn every_tool_is_in_exactly_one_group() {
        let mut seen = HashSet::new();
        for g in GROUPS {
            for t in g.tools {
                assert!(seen.insert(*t), "{t:?} is in two groups");
            }
        }
        assert_eq!(GROUPS.len(), 17);
        assert_eq!(seen.len(), 60);
    }

    #[test]
    fn remembering_a_flyout_tool_updates_its_group() {
        let mut state = ToolboxState::default();
        state.remember(Tool::Star);
        assert_eq!(state.shown[Tool::Polygon.group_index()], Tool::Star);
    }
}
