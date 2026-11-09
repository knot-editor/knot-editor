//! The foreground signal that wakes application observation; the comparison itself is
//! [`knot_model::observation`].

pub(crate) use knot_model::observation::ApplicationObservation;

struct ApplicationChanges(gpui::Entity<()>);
impl gpui::Global for ApplicationChanges {}

pub(crate) fn signal(cx: &mut gpui::App) -> gpui::Entity<()> {
    use gpui::AppContext;
    if let Some(global) = cx.try_global::<ApplicationChanges>() {
        return global.0.clone();
    }
    let signal = cx.new(|_| ());
    cx.set_global(ApplicationChanges(signal.clone()));
    signal
}

pub(crate) fn changed(cx: &mut gpui::App) {
    if let Some(signal) = cx
        .try_global::<ApplicationChanges>()
        .map(|global| global.0.clone())
    {
        signal.update(cx, |_, cx| cx.notify());
    }
}
