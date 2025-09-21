use anyhow::Result;
use global_hotkey::hotkey::HotKey;
use global_hotkey::{GlobalHotKeyEvent, GlobalHotKeyManager, HotKeyState};
use std::sync::mpsc::{self, Receiver};
use std::thread;

/// 订阅 F4 按下事件：每次按下发送一个 ()，持续有效。
pub fn subscribe_f4() -> Result<Receiver<()>> {
    use global_hotkey::hotkey::{Code, Modifiers};
    let manager: &'static mut GlobalHotKeyManager =
        Box::leak(Box::new(GlobalHotKeyManager::new()?));
    let hotkey = HotKey::new(None, Code::F4);
    let id = hotkey.id();
    manager.register(hotkey)?;
    let (tx, rx) = mpsc::channel();
    thread::spawn(move || {
        let rx_events = GlobalHotKeyEvent::receiver();
        for event in rx_events {
            if event.id == id && matches!(event.state, HotKeyState::Pressed) {
                let _ = tx.send(());
            }
        }
    });
    Ok(rx)
}
