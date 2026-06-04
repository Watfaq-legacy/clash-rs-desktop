use windows_reactor::*;

#[derive(Clone)]
pub(crate) struct SafetyState {
    pub(crate) armed_action: String,
    pub(crate) set_armed_action: SetState<String>,
}

pub(crate) fn confirm_label(
    safety: &SafetyState,
    key: &'static str,
    normal: &'static str,
    confirm: &'static str,
) -> &'static str {
    if safety.armed_action == key {
        confirm
    } else {
        normal
    }
}

pub(crate) fn guarded_action(
    safety: &SafetyState,
    key: &'static str,
    action: impl FnOnce(),
) {
    if safety.armed_action == key {
        safety.set_armed_action.call(String::new());
        action();
    } else {
        safety.set_armed_action.call(key.to_owned());
    }
}
