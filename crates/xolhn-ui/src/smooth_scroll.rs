use std::cell::RefCell;
use std::rc::Rc;
use std::time::Instant;

use gpui::{
    AnyElement, App, Bounds, DispatchPhase, Element, ElementId, Global, GlobalElementId, Hitbox,
    HitboxBehavior, HitboxId, InspectorElementId, IntoElement, LayoutId, ListState, Pixels,
    ScrollEvent, ScrollHandle, ScrollSource, UniformListScrollHandle, Window, point, px,
};

const SMOOTH_SCROLL_SPEED: f32 = 14.0;
const SNAP_THRESHOLD_PX: f32 = 0.5;
const MAX_PENDING_PX: f32 = 2400.0;
const MIN_DT_SECS: f32 = 1.0 / 144.0;
const MAX_DT_SECS: f32 = 0.05;

#[derive(Default)]
struct SmoothScrollOwner(Option<HitboxId>);

impl Global for SmoothScrollOwner {}

/// Accumulates or reverses pending smooth-scroll impulse in pixels.
pub fn enqueue_impulse(remaining_px: f32, impulse_px: f32) -> f32 {
    if !remaining_px.is_finite() || !impulse_px.is_finite() {
        return 0.0;
    }
    if remaining_px != 0.0 && impulse_px != 0.0 && remaining_px.signum() != impulse_px.signum() {
        impulse_px.clamp(-MAX_PENDING_PX, MAX_PENDING_PX)
    } else {
        (remaining_px + impulse_px).clamp(-MAX_PENDING_PX, MAX_PENDING_PX)
    }
}

/// Advances smooth-scroll decay for one frame step `dt_secs`, returning `(step_px, next_remaining_px)`.
pub fn advance_smooth_scroll(remaining_px: f32, dt_secs: f32) -> (f32, f32) {
    if !remaining_px.is_finite() {
        return (0.0, 0.0);
    }
    if remaining_px.abs() <= SNAP_THRESHOLD_PX {
        return (remaining_px, 0.0);
    }
    let dt = dt_secs.clamp(MIN_DT_SECS, MAX_DT_SECS);
    let factor = 1.0 - (-SMOOTH_SCROLL_SPEED * dt).exp();
    let step = remaining_px * factor;
    let next = remaining_px - step;
    if next.abs() <= SNAP_THRESHOLD_PX {
        (remaining_px, 0.0)
    } else {
        (step, next)
    }
}

#[derive(Debug, Default)]
struct SmoothScrollInner {
    remaining_px: f32,
    last_tick: Option<Instant>,
}

/// Shared per-scrollable state tracking remaining discrete mouse-wheel glide distance.
#[derive(Clone, Debug, Default)]
pub struct SmoothScrollState(Rc<RefCell<SmoothScrollInner>>);

impl SmoothScrollState {
    /// Creates an idle [`SmoothScrollState`].
    pub fn new() -> Self {
        Self::default()
    }

    /// Cancels any active smooth-scroll animation immediately (e.g., on programmatic jump or disable).
    pub fn cancel(&self) {
        let mut inner = self.0.borrow_mut();
        inner.remaining_px = 0.0;
        inner.last_tick = None;
    }

    /// Returns `true` when there is pending animated scroll distance.
    pub fn is_active(&self) -> bool {
        self.0.borrow().remaining_px.abs() > 0.0
    }

    fn enqueue(&self, impulse_px: f32, now: Instant) {
        let mut inner = self.0.borrow_mut();
        let was_idle = inner.remaining_px == 0.0;
        inner.remaining_px = enqueue_impulse(inner.remaining_px, impulse_px);
        if was_idle || inner.last_tick.is_none() {
            inner.last_tick = Some(now);
        }
    }

    fn step(&self, now: Instant) -> (f32, bool) {
        let mut inner = self.0.borrow_mut();
        if inner.remaining_px == 0.0 {
            inner.last_tick = None;
            return (0.0, false);
        }
        let dt_secs = inner.last_tick.replace(now).map_or(1.0 / 60.0, |prev| {
            now.saturating_duration_since(prev).as_secs_f32()
        });
        let (step, next) = advance_smooth_scroll(inner.remaining_px, dt_secs);
        inner.remaining_px = next;
        if next == 0.0 {
            inner.last_tick = None;
        }
        (step, next != 0.0)
    }
}

/// Scroll container handle driven by [`SmoothScroll`].
#[derive(Clone)]
pub enum SmoothScrollTarget {
    List(ListState),
    Scroll(ScrollHandle),
    UniformList(UniformListScrollHandle),
}

impl SmoothScrollTarget {
    fn is_scrollable(&self) -> bool {
        match self {
            Self::List(state) => state.max_offset_for_scrollbar().y > px(0.),
            Self::Scroll(handle) => handle.max_offset().y > px(0.),
            Self::UniformList(handle) => {
                let base_handle = handle.0.borrow().base_handle.clone();
                base_handle.max_offset().y > px(0.)
            }
        }
    }

    fn is_dragging_scrollbar(&self) -> bool {
        match self {
            Self::List(state) => state.is_scrollbar_dragging(),
            Self::Scroll(_) | Self::UniformList(_) => false,
        }
    }

    fn line_height(&self, window: &Window) -> Pixels {
        match self {
            // Matches `gpui::List::paint`'s `pixel_delta(px(20.))` multiplier.
            Self::List(_) => px(20.),
            Self::Scroll(_) | Self::UniformList(_) => window.line_height(),
        }
    }

    fn apply_delta(&self, step_px: f32) -> bool {
        match self {
            Self::List(state) => {
                let current_top = -f32::from(state.scroll_px_offset_for_scrollbar().y);
                let max_top = f32::from(state.max_offset_for_scrollbar().y).max(0.0);
                if step_px < 0.0 && current_top <= 0.01 {
                    return false;
                }
                if step_px > 0.0 && (max_top <= 0.0 || state.is_scrolled_to_end() == Some(true)) {
                    return false;
                }
                state.scroll_by(px(step_px));
                true
            }
            Self::Scroll(handle) => apply_scroll_handle_delta(handle, step_px),
            Self::UniformList(handle) => {
                let base_handle = handle.0.borrow().base_handle.clone();
                apply_scroll_handle_delta(&base_handle, step_px)
            }
        }
    }
}

fn apply_scroll_handle_delta(handle: &ScrollHandle, step_px: f32) -> bool {
    let offset = handle.offset();
    let max_y = f32::from(handle.max_offset().y).max(0.0);
    if max_y <= 0.0 {
        return false;
    }
    let current_y = f32::from(offset.y);
    let next_y = (current_y - step_px).clamp(-max_y, 0.0);
    if (next_y - current_y).abs() < 0.01 {
        return false;
    }
    handle.set_offset(point(offset.x, px(next_y)));
    true
}

/// Wraps a scrollable element to animate discrete mouse-wheel scrolling when `enabled` is true
/// and system `reduce_motion` is off. Trackpad and touch gestures remain untouched.
pub fn smooth_scroll(
    child: impl IntoElement,
    target: SmoothScrollTarget,
    state: &SmoothScrollState,
    enabled: bool,
) -> SmoothScroll {
    SmoothScroll {
        child: child.into_any_element(),
        target,
        state: state.clone(),
        enabled,
    }
}

/// Pass-through wrapper element driving frame-rate-independent mouse-wheel scroll smoothing.
pub struct SmoothScroll {
    child: AnyElement,
    target: SmoothScrollTarget,
    state: SmoothScrollState,
    enabled: bool,
}

impl IntoElement for SmoothScroll {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

impl Element for SmoothScroll {
    type RequestLayoutState = ();
    type PrepaintState = Hitbox;

    fn id(&self) -> Option<ElementId> {
        None
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
        if !self.enabled
            || cx.reduce_motion()
            || self.target.is_dragging_scrollbar()
            || !self.target.is_scrollable()
        {
            self.state.cancel();
        } else if self.state.is_active() {
            let (step_px, still_active) = self.state.step(Instant::now());
            if step_px.abs() > 0.0 {
                if !self.target.apply_delta(step_px) {
                    self.state.cancel();
                } else if still_active {
                    window.request_animation_frame();
                }
            } else if still_active {
                window.request_animation_frame();
            }
        }

        let hitbox = window.insert_hitbox(bounds, HitboxBehavior::Normal);
        self.child.prepaint(window, cx);
        hitbox
    }

    fn paint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        _bounds: Bounds<Pixels>,
        _: &mut Self::RequestLayoutState,
        prepaint: &mut Self::PrepaintState,
        window: &mut Window,
        cx: &mut App,
    ) {
        let active = self.enabled && !cx.reduce_motion();
        let hitbox_id = prepaint.id;

        if active {
            let target_claim = self.target.clone();
            window.on_mouse_event(move |event: &ScrollEvent, phase, window, cx| {
                if phase == DispatchPhase::Capture
                    && hitbox_id.should_handle_scroll(window)
                    && !event.delta.precise()
                    && event.source == ScrollSource::Pointer
                    && target_claim.is_scrollable()
                    && !target_claim.is_dragging_scrollbar()
                {
                    let line_height = target_claim.line_height(window);
                    let pixel_delta = event.delta.pixel_delta(line_height);
                    if pixel_delta.y != px(0.) && pixel_delta.x.abs() <= pixel_delta.y.abs() {
                        cx.default_global::<SmoothScrollOwner>().0 = Some(hitbox_id);
                    }
                }
            });
        }

        self.child.paint(window, cx);

        if active {
            let target = self.target.clone();
            let state = self.state.clone();
            let view = window.current_view();
            window.on_mouse_event(move |event: &ScrollEvent, phase, window, cx| {
                if phase != DispatchPhase::Bubble
                    || !hitbox_id.should_handle_scroll(window)
                    || !cx
                        .try_global::<SmoothScrollOwner>()
                        .is_some_and(|owner| owner.0 == Some(hitbox_id))
                    || event.delta.precise()
                    || event.source != ScrollSource::Pointer
                {
                    return;
                }

                cx.default_global::<SmoothScrollOwner>().0 = None;

                let line_height = target.line_height(window);
                let pixel_delta = event.delta.pixel_delta(line_height);
                if pixel_delta.y == px(0.)
                    || pixel_delta.x.abs() > pixel_delta.y.abs()
                    || !target.is_scrollable()
                    || target.is_dragging_scrollbar()
                {
                    return;
                }

                let impulse = -f32::from(pixel_delta.y);
                state.enqueue(impulse, Instant::now());
                cx.stop_propagation();
                cx.notify(view);
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn advance_smooth_scroll_decays_monotonically_and_snaps_to_zero() {
        for dt in [1.0 / 60.0, 1.0 / 120.0] {
            let mut rem = 240.0_f32;
            let mut total = 0.0_f32;
            let mut prev_step = f32::MAX;
            let mut steps = 0usize;

            while rem != 0.0 {
                let (step, next) = advance_smooth_scroll(rem, dt);
                assert!(step > 0.0);
                if next > 0.0 {
                    assert!(step <= prev_step + 1e-4);
                }
                prev_step = step;
                total += step;
                rem = next;
                steps += 1;
                assert!(steps < 200, "scroll did not terminate cleanly");
            }

            assert!((total - 240.0).abs() < 1e-3);
        }

        assert_eq!(advance_smooth_scroll(0.4, 1.0 / 60.0), (0.4, 0.0));
        assert_eq!(advance_smooth_scroll(f32::NAN, 1.0 / 60.0), (0.0, 0.0));
    }

    #[test]
    fn enqueue_impulse_accumulates_same_direction_and_reverses_immediately() {
        assert_eq!(enqueue_impulse(100.0, 60.0), 160.0);
        assert_eq!(enqueue_impulse(2300.0, 300.0), MAX_PENDING_PX);
        assert_eq!(enqueue_impulse(-2300.0, -300.0), -MAX_PENDING_PX);
        // Opposite sign reverses immediately instead of cancelling partially.
        assert_eq!(enqueue_impulse(120.0, -60.0), -60.0);
        assert_eq!(enqueue_impulse(-120.0, 60.0), 60.0);
    }

    #[test]
    fn smooth_scroll_state_cancel_clears_active_glide() {
        let state = SmoothScrollState::new();
        assert!(!state.is_active());

        let t0 = Instant::now();
        state.enqueue(120.0, t0);
        assert!(state.is_active());

        let (step, still_active) = state.step(t0 + Duration::from_millis(16));
        assert!(step > 0.0);
        assert!(still_active);
        assert!(state.is_active());

        state.cancel();
        assert!(!state.is_active());
        assert_eq!(state.step(t0 + Duration::from_millis(32)), (0.0, false));
    }
}
