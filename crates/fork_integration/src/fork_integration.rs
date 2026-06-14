use gpui::App;

pub fn init(cx: &mut App) {
    thread_switcher::init(cx);
    pr_indicator::init(cx);
}
