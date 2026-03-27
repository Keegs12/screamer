use crate::config::OverlayPosition;
use objc2::msg_send;
use objc2::rc::Retained;
use objc2_app_kit::{
    NSBackingStoreType, NSColor, NSPanel, NSScreen, NSView, NSVisualEffectMaterial,
    NSVisualEffectView, NSWindowStyleMask,
};
use objc2_core_foundation::{CGFloat, CGPoint, CGRect, CGSize};
use objc2_foundation::MainThreadMarker;

pub const WAVEFORM_BINS: usize = 64;

const WINDOW_WIDTH: f64 = 340.0;
const WINDOW_HEIGHT: f64 = 80.0;
const NUM_BARS: usize = WAVEFORM_BINS;
const BAR_WIDTH: f64 = 2.3;
const BAR_SPACING: f64 = 2.6;
const BAR_MIN_HEIGHT: f64 = 2.0;
const CORNER_RADIUS: f64 = 18.0;
const PADDING_X: f64 = 14.0;
const PADDING_Y: f64 = 9.0;
const BAR_MAX_HEIGHT: f64 = WINDOW_HEIGHT - PADDING_Y * 2.0;

const POSITION_MARGIN: f64 = 40.0;

pub struct Overlay {
    panel: Retained<NSPanel>,
    bar_views: Vec<Retained<NSView>>,
    current_heights: [f64; NUM_BARS],
    visible: bool,
    position: OverlayPosition,
}

fn waveform_level_for_bar(bar_idx: usize, waveform: &[f32]) -> f64 {
    if waveform.is_empty() {
        return 0.0;
    }

    let sample_idx = bar_idx * waveform.len() / NUM_BARS;
    waveform[sample_idx.min(waveform.len() - 1)].clamp(0.0, 1.0) as f64
}

fn smooth_height(current: f64, target: f64) -> f64 {
    let smoothing = if target > current { 0.62 } else { 0.36 };
    let next = current + (target - current) * smoothing;
    next.max(BAR_MIN_HEIGHT).min(BAR_MAX_HEIGHT)
}

impl Overlay {
    pub fn new(mtm: MainThreadMarker, position: OverlayPosition) -> Self {
        let style = NSWindowStyleMask::Borderless | NSWindowStyleMask::NonactivatingPanel;

        let frame = CGRect::new(
            CGPoint::new(0.0, 0.0),
            CGSize::new(WINDOW_WIDTH, WINDOW_HEIGHT),
        );

        let panel = {
            let panel = NSPanel::initWithContentRect_styleMask_backing_defer(
                mtm.alloc::<NSPanel>(),
                frame,
                style,
                NSBackingStoreType::Buffered,
                false,
            );
            panel.setLevel(25);
            panel.setOpaque(false);
            panel.setBackgroundColor(Some(&NSColor::clearColor()));
            panel.setHasShadow(true);
            panel.setMovableByWindowBackground(false);
            panel.setHidesOnDeactivate(false);
            panel.setAlphaValue(0.0);
            panel.setCollectionBehavior(
                objc2_app_kit::NSWindowCollectionBehavior::CanJoinAllSpaces
                    | objc2_app_kit::NSWindowCollectionBehavior::Stationary
                    | objc2_app_kit::NSWindowCollectionBehavior::IgnoresCycle,
            );
            panel
        };

        // Frosted glass background
        let effect_view = {
            let view = NSVisualEffectView::new(mtm);
            view.setFrame(frame);
            view.setMaterial(NSVisualEffectMaterial::HUDWindow);
            view.setBlendingMode(objc2_app_kit::NSVisualEffectBlendingMode::BehindWindow);
            view.setState(objc2_app_kit::NSVisualEffectState::Active);
            view.setWantsLayer(true);
            if let Some(layer) = view.layer() {
                layer.setCornerRadius(CORNER_RADIUS as CGFloat);
                layer.setMasksToBounds(true);
            }
            view
        };

        let usable_width = WINDOW_WIDTH - PADDING_X * 2.0;
        let total_bars_width = NUM_BARS as f64 * BAR_WIDTH + (NUM_BARS - 1) as f64 * BAR_SPACING;
        let bars_x_offset = PADDING_X + (usable_width - total_bars_width) / 2.0;

        let mut bar_views = Vec::with_capacity(NUM_BARS);
        for i in 0..NUM_BARS {
            let x = bars_x_offset + i as f64 * (BAR_WIDTH + BAR_SPACING);
            let bar = {
                let view = NSView::new(mtm);
                let y = (WINDOW_HEIGHT - BAR_MIN_HEIGHT) / 2.0;
                view.setFrame(CGRect::new(
                    CGPoint::new(x, y),
                    CGSize::new(BAR_WIDTH, BAR_MIN_HEIGHT),
                ));
                view.setWantsLayer(true);
                if let Some(layer) = view.layer() {
                    let dist_from_center = ((i as f64) - (NUM_BARS - 1) as f64 / 2.0).abs()
                        / ((NUM_BARS - 1) as f64 / 2.0);
                    let glow = 1.0 - dist_from_center * 0.45;
                    let r = 0.98;
                    let g = 0.80 + 0.16 * glow;
                    let b = 0.22 + 0.08 * glow;
                    let alpha = 0.40 + 0.24 * glow;
                    let ns_color = NSColor::colorWithRed_green_blue_alpha(r, g, b, alpha);
                    unsafe {
                        let cg_color: *const std::ffi::c_void = msg_send![&ns_color, CGColor];
                        let _: () = msg_send![&*layer, setBackgroundColor: cg_color];
                    }
                    layer.setCornerRadius((BAR_WIDTH / 2.0) as CGFloat);
                }
                view
            };
            bar_views.push(bar);
        }

        {
            let content_view = panel.contentView().unwrap();
            content_view.addSubview(&effect_view);
            for bar in &bar_views {
                effect_view.addSubview(bar);
            }
        }

        let s = Self {
            panel,
            bar_views,
            current_heights: [BAR_MIN_HEIGHT; NUM_BARS],
            visible: false,
            position,
        };
        s.apply_position(mtm);
        s
    }

    pub fn show(&mut self) {
        self.visible = true;
        self.current_heights = [BAR_MIN_HEIGHT; NUM_BARS];
        self.panel.orderFrontRegardless();
        self.panel.setAlphaValue(1.0);
    }

    pub fn hide(&mut self) {
        self.visible = false;
        self.current_heights = [BAR_MIN_HEIGHT; NUM_BARS];
        self.panel.setAlphaValue(0.0);
        self.panel.orderOut(None);
    }

    pub fn update_waveform(&mut self, waveform: &[f32]) {
        if !self.visible {
            return;
        }

        for bar_idx in 0..NUM_BARS {
            let level = waveform_level_for_bar(bar_idx, waveform);
            let target = BAR_MIN_HEIGHT + level * (BAR_MAX_HEIGHT - BAR_MIN_HEIGHT);
            self.current_heights[bar_idx] = smooth_height(self.current_heights[bar_idx], target);

            let h = self.current_heights[bar_idx];
            let y = (WINDOW_HEIGHT - h) / 2.0;

            let mut frame: CGRect = self.bar_views[bar_idx].frame();
            frame.origin.y = y as CGFloat;
            frame.size.height = h as CGFloat;
            self.bar_views[bar_idx].setFrame(frame);
        }
    }

    pub fn is_visible(&self) -> bool {
        self.visible
    }

    pub fn set_position(&mut self, mtm: MainThreadMarker, position: OverlayPosition) {
        self.position = position;
        self.apply_position(mtm);
    }

    fn apply_position(&self, mtm: MainThreadMarker) {
        if let Some(screen) = NSScreen::mainScreen(mtm) {
            let sf = screen.frame();
            let x = (sf.size.width - WINDOW_WIDTH) / 2.0;
            let y = match self.position {
                OverlayPosition::Center => (sf.size.height - WINDOW_HEIGHT) / 2.0,
                OverlayPosition::Top => sf.size.height - WINDOW_HEIGHT - POSITION_MARGIN,
                OverlayPosition::Bottom => POSITION_MARGIN,
            };
            self.panel.setFrameOrigin(CGPoint::new(x, y));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn smoothing_moves_toward_target() {
        let next = smooth_height(BAR_MIN_HEIGHT, BAR_MAX_HEIGHT);
        assert!(next > BAR_MIN_HEIGHT);
    }

    #[test]
    fn smoothing_flattens_toward_silence() {
        let next = smooth_height(BAR_MAX_HEIGHT, BAR_MIN_HEIGHT);
        assert!(next < BAR_MAX_HEIGHT);
    }

    #[test]
    fn waveform_mapping_preserves_left_to_right_order() {
        let waveform = [0.0, 0.2, 0.4, 1.0];
        assert_eq!(waveform_level_for_bar(0, &waveform), 0.0);
        assert!(waveform_level_for_bar(NUM_BARS - 1, &waveform) > 0.9);
    }
}
