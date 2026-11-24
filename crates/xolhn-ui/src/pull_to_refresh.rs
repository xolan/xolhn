use std::cell::RefCell;
use std::rc::Rc;
use std::time::Instant;

use gpui::{
    AnyElement, App, AvailableSpace, Bounds, ContentMask, DispatchPhase, Element, ElementId,
    FontWeight, GlobalElementId, Hitbox, HitboxBehavior, InspectorElementId, IntoElement, LayoutId,
    ListState, ParentElement as _, Pixels, ScrollDelta, ScrollEvent, ScrollSource, Styled as _,
    TouchPhase, Window, div, point, px, rems, size,
};
use gpui_kit::component::spinner::Spinner;
use gpui_kit::component::{Icon, IconName, Sizable as _, h_flex};

use crate::theme_bridge::UiColors;
use crate::views::ActionCallback;

pub const TRIGGER_PULL_PX: f32 = 56.0;
pub const MAX_VISUAL_PULL_PX: f32 = 88.0;
pub const RUBBER_BAND_TRACKING: f32 = 0.85;
pub const REFRESHING_HOLD_PX: f32 = 48.0;
pub const INDICATOR_HEIGHT_PX: f32 = 44.0;
const RETRACT_SPEED: f32 = 16.0;
const SNAP_THRESHOLD_PX: f32 = 0.5;
const MIN_DT_SECS: f32 = 1.0 / 144.0;
const MAX_DT_SECS: f32 = 0.05;

/// Computes rubber-band displaced height in pixels for a raw downward pull distance.
pub fn rubber_band_pull(raw_pull_px: f32) -> f32 {
    if !raw_pull_px.is_finite() || raw_pull_px <= 0.0 {
        return 0.0;
    }
    let normalized = (raw_pull_px * RUBBER_BAND_TRACKING) / MAX_VISUAL_PULL_PX;
    MAX_VISUAL_PULL_PX * (1.0 - (-normalized).exp())
}

/// Steps `current_px` smoothly toward `target_px`, returning `(next_px, still_animating)`.
pub fn step_offset_toward(
    current_px: f32,
    target_px: f32,
    dt_secs: f32,
    reduce_motion: bool,
) -> (f32, bool) {
    if !current_px.is_finite() || !target_px.is_finite() {
        return (0.0, false);
    }
    let diff = target_px - current_px;
    if diff.abs() <= SNAP_THRESHOLD_PX || reduce_motion {
        return (target_px, false);
    }
    let dt = dt_secs.clamp(MIN_DT_SECS, MAX_DT_SECS);
    let factor = 1.0 - (-RETRACT_SPEED * dt).exp();
    let next = current_px + diff * factor;
    if (target_px - next).abs() <= SNAP_THRESHOLD_PX {
        (target_px, false)
    } else {
        (next, true)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PullGestureOutcome {
    PassThrough,
    Consumed { trigger_refresh: bool },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PullVisualPhase {
    Idle,
    Pulling { armed: bool },
    Refreshing,
}

#[derive(Debug, Default)]
struct PullToRefreshInner {
    finger_down: bool,
    raw_pull_px: f32,
    visual_offset_px: f32,
    last_tick: Option<Instant>,
    pending_refresh_frames: u8,
}

/// Shared per-scrollable state driving overscroll pull-to-refresh gestures and indicator bounce.
#[derive(Clone, Debug, Default)]
pub struct PullToRefreshState(Rc<RefCell<PullToRefreshInner>>);

impl PullToRefreshState {
    /// Creates an idle [`PullToRefreshState`].
    pub fn new() -> Self {
        Self::default()
    }

    /// Resets active pull tracking and snaps indicator state back to rest.
    pub fn reset(&self) {
        *self.0.borrow_mut() = PullToRefreshInner::default();
    }

    /// Processes one touch scroll packet and returns whether pull-to-refresh consumed it.
    pub fn on_scroll_step(
        &self,
        delta_x: f32,
        delta_y: f32,
        phase: TouchPhase,
        is_at_top: bool,
        refreshing: bool,
        now: Instant,
    ) -> PullGestureOutcome {
        let mut inner = self.0.borrow_mut();
        if refreshing {
            inner.pending_refresh_frames = 0;
        }

        match phase {
            TouchPhase::Started | TouchPhase::Moved => {
                if phase == TouchPhase::Started {
                    inner.finger_down = true;
                    inner.last_tick = Some(now);
                } else if !inner.finger_down {
                    // Ignore post-fling momentum packets emitted after finger release.
                    return PullGestureOutcome::PassThrough;
                }

                if refreshing {
                    inner.raw_pull_px = 0.0;
                    return PullGestureOutcome::PassThrough;
                }

                if !delta_x.is_finite() || !delta_y.is_finite() {
                    return PullGestureOutcome::PassThrough;
                }
                if delta_x.abs() > delta_y.abs() {
                    return PullGestureOutcome::PassThrough;
                }

                let pulling_down = delta_y > 0.0 && (inner.raw_pull_px > 0.0 || is_at_top);
                let pushing_back = delta_y < 0.0 && inner.raw_pull_px > 0.0;
                if pulling_down || pushing_back {
                    inner.raw_pull_px = (inner.raw_pull_px + delta_y).max(0.0);
                    inner.visual_offset_px = rubber_band_pull(inner.raw_pull_px);
                    inner.last_tick = Some(now);
                    PullGestureOutcome::Consumed {
                        trigger_refresh: false,
                    }
                } else {
                    PullGestureOutcome::PassThrough
                }
            }
            TouchPhase::Ended => {
                let was_down = std::mem::replace(&mut inner.finger_down, false);
                if !was_down {
                    return PullGestureOutcome::PassThrough;
                }
                if inner.raw_pull_px > 0.0 {
                    let armed = inner.visual_offset_px >= TRIGGER_PULL_PX && !refreshing;
                    inner.raw_pull_px = 0.0;
                    inner.last_tick = Some(now);
                    if armed {
                        inner.pending_refresh_frames = 2;
                    }
                    PullGestureOutcome::Consumed {
                        trigger_refresh: armed,
                    }
                } else {
                    PullGestureOutcome::PassThrough
                }
            }
            TouchPhase::Cancelled => {
                let had_pull = inner.raw_pull_px > 0.0;
                inner.finger_down = false;
                inner.raw_pull_px = 0.0;
                inner.last_tick = Some(now);
                if had_pull {
                    PullGestureOutcome::Consumed {
                        trigger_refresh: false,
                    }
                } else {
                    PullGestureOutcome::PassThrough
                }
            }
        }
    }

    fn prepare_frame(
        &self,
        refreshing: bool,
        reduce_motion: bool,
        now: Instant,
    ) -> (f32, PullVisualPhase, bool) {
        let mut inner = self.0.borrow_mut();
        if refreshing {
            inner.pending_refresh_frames = 0;
        } else if inner.pending_refresh_frames > 0 {
            inner.pending_refresh_frames -= 1;
        }

        let hold_refresh = refreshing || inner.pending_refresh_frames > 0;
        if inner.finger_down && !refreshing {
            let offset = inner.visual_offset_px;
            let phase = if offset > SNAP_THRESHOLD_PX {
                PullVisualPhase::Pulling {
                    armed: offset >= TRIGGER_PULL_PX,
                }
            } else {
                PullVisualPhase::Idle
            };
            return (offset, phase, false);
        }

        let target_px = if hold_refresh {
            REFRESHING_HOLD_PX
        } else {
            0.0
        };
        let dt_secs = inner.last_tick.replace(now).map_or(1.0 / 60.0, |prev| {
            now.saturating_duration_since(prev).as_secs_f32()
        });
        let (next_px, still_animating) =
            step_offset_toward(inner.visual_offset_px, target_px, dt_secs, reduce_motion);
        inner.visual_offset_px = next_px;
        if !still_animating {
            inner.last_tick = None;
        }

        let phase = if hold_refresh {
            PullVisualPhase::Refreshing
        } else if next_px > SNAP_THRESHOLD_PX {
            PullVisualPhase::Pulling {
                armed: next_px >= TRIGGER_PULL_PX,
            }
        } else {
            PullVisualPhase::Idle
        };

        (next_px, phase, still_animating)
    }
}

/// Scroll target interrogated to check whether the viewport is currently at its top edge.
#[derive(Clone)]
pub enum PullToRefreshTarget {
    List(ListState),
    AlwaysAtTop,
}

impl PullToRefreshTarget {
    fn is_at_top(&self) -> bool {
        match self {
            Self::List(state) => {
                let top = state.logical_scroll_top();
                top.item_ix == 0 && f32::from(top.offset_in_item) <= 0.5
            }
            Self::AlwaysAtTop => true,
        }
    }
}

/// Wraps a scrollable list or panel with touch pull-to-refresh behavior and top indicator header.
#[allow(clippy::too_many_arguments)]
pub fn pull_to_refresh(
    id: impl Into<ElementId>,
    child: impl IntoElement,
    target: PullToRefreshTarget,
    state: &PullToRefreshState,
    refreshing: bool,
    colors: &UiColors,
    on_refresh: ActionCallback,
) -> PullToRefresh {
    PullToRefresh {
        id: id.into(),
        child: child.into_any_element(),
        target,
        state: state.clone(),
        refreshing,
        colors: *colors,
        on_refresh,
    }
}

/// Pass-through element rendering a pull-to-refresh indicator above displaced scrollable content.
pub struct PullToRefresh {
    id: ElementId,
    child: AnyElement,
    target: PullToRefreshTarget,
    state: PullToRefreshState,
    refreshing: bool,
    colors: UiColors,
    on_refresh: ActionCallback,
}

pub struct PullToRefreshPrepaint {
    hitbox: Hitbox,
    indicator: Option<AnyElement>,
}

impl IntoElement for PullToRefresh {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

impl Element for PullToRefresh {
    type RequestLayoutState = ();
    type PrepaintState = PullToRefreshPrepaint;

    fn id(&self) -> Option<ElementId> {
        Some(self.id.clone())
    }

    fn source_location(&self) -> Option<&'static core::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, Self::RequestLayoutState) {
        (self.child.request_layout(window, cx), ())
    }

    fn prepaint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _: &mut Self::RequestLayoutState,
        window: &mut Window,
        cx: &mut App,
    ) -> Self::PrepaintState {
        let (visual_offset, phase, still_animating) =
            self.state
                .prepare_frame(self.refreshing, cx.reduce_motion(), Instant::now());
        if still_animating {
            window.request_animation_frame();
        }

        let hitbox = window.insert_hitbox(bounds, HitboxBehavior::Normal);

        window.with_content_mask(Some(ContentMask { bounds }), |window| {
            window.with_element_offset(point(px(0.), px(visual_offset)), |window| {
                self.child.prepaint(window, cx);
            });
        });

        let indicator = if visual_offset > SNAP_THRESHOLD_PX && phase != PullVisualPhase::Idle {
            let mut el = render_indicator(phase, &self.colors).into_any_element();
            let available = size(
                AvailableSpace::Definite(bounds.size.width),
                AvailableSpace::Definite(px(INDICATOR_HEIGHT_PX)),
            );
            el.layout_as_root(available, window, cx);
            let indicator_y = if visual_offset < INDICATOR_HEIGHT_PX {
                visual_offset - INDICATOR_HEIGHT_PX
            } else {
                (visual_offset - INDICATOR_HEIGHT_PX) * 0.5
            };
            window.with_content_mask(Some(ContentMask { bounds }), |window| {
                el.prepaint_at(bounds.origin + point(px(0.), px(indicator_y)), window, cx);
            });
            Some(el)
        } else {
            None
        };

        PullToRefreshPrepaint { hitbox, indicator }
    }

    fn paint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _: &mut Self::RequestLayoutState,
        prepaint: &mut Self::PrepaintState,
        window: &mut Window,
        cx: &mut App,
    ) {
        window.with_content_mask(Some(ContentMask { bounds }), |window| {
            self.child.paint(window, cx);
            if let Some(indicator) = &mut prepaint.indicator {
                indicator.paint(window, cx);
            }
        });

        let hitbox_id = prepaint.hitbox.id;
        let target = self.target.clone();
        let state = self.state.clone();
        let refreshing = self.refreshing;
        let on_refresh = self.on_refresh.clone();
        let view = window.current_view();

        window.on_mouse_event(move |event: &ScrollEvent, phase, window, cx| {
            if phase != DispatchPhase::Bubble
                || !hitbox_id.should_handle_scroll(window)
                || event.source != ScrollSource::Touch
            {
                return;
            }
            let ScrollDelta::Pixels(delta) = event.delta else {
                return;
            };

            let outcome = state.on_scroll_step(
                f32::from(delta.x),
                f32::from(delta.y),
                event.touch_phase,
                target.is_at_top(),
                refreshing,
                Instant::now(),
            );

            if let PullGestureOutcome::Consumed { trigger_refresh } = outcome {
                if trigger_refresh {
                    on_refresh(window, cx);
                }
                cx.stop_propagation();
                cx.notify(view);
            }
        });
    }
}

fn render_indicator(phase: PullVisualPhase, colors: &UiColors) -> AnyElement {
    let content = match phase {
        PullVisualPhase::Idle => div().into_any_element(),
        PullVisualPhase::Pulling { armed } => {
            let text_color = if armed { colors.accent } else { colors.muted };
            let icon_name = if armed {
                IconName::RefreshCw
            } else {
                IconName::ArrowDown
            };
            let label = if armed {
                "Release to refresh"
            } else {
                "Pull to refresh"
            };

            h_flex()
                .items_center()
                .gap_2()
                .px_3()
                .py_1()
                .rounded_full()
                .bg(if armed {
                    colors.accent_soft
                } else {
                    colors.surface_2
                })
                .border_1()
                .border_color(if armed { colors.accent } else { colors.border })
                .child(Icon::new(icon_name).small().text_color(text_color))
                .child(
                    div()
                        .text_size(rems(0.76))
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(text_color)
                        .child(label),
                )
                .into_any_element()
        }
        PullVisualPhase::Refreshing => h_flex()
            .items_center()
            .gap_2()
            .px_3()
            .py_1()
            .rounded_full()
            .bg(colors.surface_2)
            .border_1()
            .border_color(colors.border)
            .child(Spinner::new().small().color(colors.accent))
            .child(
                div()
                    .text_size(rems(0.76))
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(colors.text)
                    .child("Refreshing…"),
            )
            .into_any_element(),
    };

    h_flex()
        .w_full()
        .h(px(INDICATOR_HEIGHT_PX))
        .items_center()
        .justify_center()
        .child(content)
        .into_any_element()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn rubber_band_pull_is_monotonic_bounded_and_reaches_trigger() {
        assert_eq!(rubber_band_pull(0.0), 0.0);
        assert_eq!(rubber_band_pull(-20.0), 0.0);
        assert_eq!(rubber_band_pull(f32::NAN), 0.0);

        let a = rubber_band_pull(40.0);
        let b = rubber_band_pull(120.0);
        let c = rubber_band_pull(10_000.0);
        assert!(a > 0.0 && a < b);
        assert!(b >= TRIGGER_PULL_PX);
        assert!(c > b && c <= MAX_VISUAL_PULL_PX);
    }

    #[test]
    fn step_offset_toward_converges_and_honors_reduce_motion() {
        assert_eq!(
            step_offset_toward(80.0, 0.0, 1.0 / 60.0, true),
            (0.0, false)
        );
        assert_eq!(
            step_offset_toward(12.0, REFRESHING_HOLD_PX, 1.0 / 60.0, true),
            (REFRESHING_HOLD_PX, false)
        );

        let mut cur = 72.0_f32;
        let mut steps = 0usize;
        loop {
            let (next, active) = step_offset_toward(cur, 0.0, 1.0 / 60.0, false);
            assert!(next <= cur);
            cur = next;
            steps += 1;
            assert!(steps < 200);
            if !active {
                break;
            }
        }
        assert_eq!(cur, 0.0);
    }

    #[test]
    fn pull_gesture_triggers_only_on_armed_release_and_ignores_momentum() {
        let state = PullToRefreshState::new();
        let t0 = Instant::now();

        // Post-fling momentum without a preceding Started must never start a pull.
        assert_eq!(
            state.on_scroll_step(0.0, 120.0, TouchPhase::Moved, true, false, t0),
            PullGestureOutcome::PassThrough
        );

        // Short pull below trigger threshold retracts without refreshing.
        assert_eq!(
            state.on_scroll_step(0.0, 20.0, TouchPhase::Started, true, false, t0),
            PullGestureOutcome::Consumed {
                trigger_refresh: false
            }
        );
        assert_eq!(
            state.on_scroll_step(
                0.0,
                0.0,
                TouchPhase::Ended,
                true,
                false,
                t0 + Duration::from_millis(50)
            ),
            PullGestureOutcome::Consumed {
                trigger_refresh: false
            }
        );

        // Pulling down away from top passes through to regular list scroll.
        assert_eq!(
            state.on_scroll_step(
                0.0,
                160.0,
                TouchPhase::Started,
                false,
                false,
                t0 + Duration::from_millis(100)
            ),
            PullGestureOutcome::PassThrough
        );
        assert_eq!(
            state.on_scroll_step(
                0.0,
                0.0,
                TouchPhase::Ended,
                false,
                false,
                t0 + Duration::from_millis(150)
            ),
            PullGestureOutcome::PassThrough
        );

        // Pulling far enough at top arms and fires refresh on release.
        assert_eq!(
            state.on_scroll_step(
                0.0,
                140.0,
                TouchPhase::Started,
                true,
                false,
                t0 + Duration::from_millis(200)
            ),
            PullGestureOutcome::Consumed {
                trigger_refresh: false
            }
        );
        assert_eq!(
            state.on_scroll_step(
                0.0,
                0.0,
                TouchPhase::Ended,
                true,
                false,
                t0 + Duration::from_millis(250)
            ),
            PullGestureOutcome::Consumed {
                trigger_refresh: true
            }
        );

        let (offset, phase, _) = state.prepare_frame(true, false, t0 + Duration::from_millis(266));
        assert_eq!(phase, PullVisualPhase::Refreshing);
        assert!(offset >= 0.0);
    }
}
