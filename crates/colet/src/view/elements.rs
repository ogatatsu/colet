//! Elements of Colet's own: one drawn over what follows it, clipped where it is.
use super::*;

/// Draws its child over everything laid out after it, as `deferred` does, but clipped where it is, for
/// presses as for paint: a column scrolled under home's pane keeps its edge there.
pub(super) fn on_top(child: impl IntoElement) -> OnTop {
    let clip = Rc::new(std::cell::Cell::new(None));
    OnTop(Some(Clipped { clip: clip.clone(), child: child.into_any_element() }.into_any_element()), clip)
}

pub(super) struct OnTop(Option<AnyElement>, Rc<std::cell::Cell<Option<ContentMask<Pixels>>>>);

impl Element for OnTop {
    type RequestLayoutState = ();
    type PrepaintState = ();

    fn id(&self) -> Option<ElementId> {
        None
    }

    fn source_location(&self) -> Option<&'static core::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, ()) {
        (self.0.as_mut().unwrap().request_layout(window, cx), ())
    }

    fn prepaint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        _: Bounds<Pixels>,
        _: &mut (),
        window: &mut Window,
        _: &mut App,
    ) {
        let (offset, mask) = (window.element_offset(), window.content_mask());
        self.1.set(Some(mask));
        window.defer_draw(self.0.take().unwrap(), offset, 0, Some(mask));
    }

    fn paint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        _: Bounds<Pixels>,
        _: &mut (),
        _: &mut (),
        _: &mut Window,
        _: &mut App,
    ) {
    }
}

impl IntoElement for OnTop {
    type Element = Self;

    fn into_element(self) -> Self {
        self
    }
}

/// What [`OnTop`] defers: GPUI clips a deferred draw's paint, but not the hit areas laid out ahead of it.
struct Clipped {
    clip: Rc<std::cell::Cell<Option<ContentMask<Pixels>>>>,
    child: AnyElement,
}

impl Element for Clipped {
    type RequestLayoutState = ();
    type PrepaintState = ();

    fn id(&self) -> Option<ElementId> {
        None
    }

    fn source_location(&self) -> Option<&'static core::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, ()) {
        (self.child.request_layout(window, cx), ())
    }

    fn prepaint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        _: Bounds<Pixels>,
        _: &mut (),
        window: &mut Window,
        cx: &mut App,
    ) {
        window.with_content_mask(self.clip.get(), |window| self.child.prepaint(window, cx));
    }

    fn paint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        _: Bounds<Pixels>,
        _: &mut (),
        _: &mut (),
        window: &mut Window,
        cx: &mut App,
    ) {
        window.with_content_mask(self.clip.get(), |window| self.child.paint(window, cx));
    }
}

impl IntoElement for Clipped {
    type Element = Self;

    fn into_element(self) -> Self {
        self
    }
}
