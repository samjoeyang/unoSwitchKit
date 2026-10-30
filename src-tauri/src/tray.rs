use tauri::tray::{MouseButton, TrayIconBuilder, TrayIconEvent};
use tauri::{Manager, PhysicalPosition, PhysicalSize};

/// 创建菜单栏托盘，左键点击将无边框面板定位到图标正下方居中。
pub fn create_tray(app: &tauri::App) -> Result<(), Box<dyn std::error::Error>> {
    // 单色开关 glyph，编译期内嵌，避免运行时路径问题
    let icon = tauri::image::Image::from_bytes(include_bytes!("../icons/tray-icon.png"))
        .expect("加载托盘图标失败");

    TrayIconBuilder::with_id("main-tray")
        .icon(icon)
        // 模板图：macOS 自动按菜单栏深/浅色渲染，与系统图标观感一致
        .icon_as_template(true)
        .tooltip("SwitchKit")
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                ..
            } = event
            {
                let app = tray.app_handle();
                if let Ok(Some(rect)) = tray.rect() {
                    if let (tauri::Position::Physical(pos), tauri::Size::Physical(size)) =
                        (rect.position, rect.size)
                    {
                        position_and_show(&app, pos, size);
                    }
                }
            }
        })
        .build(app)?;
    Ok(())
}

/// 将面板定位到托盘图标下方并弹出。
fn position_and_show(
    app: &tauri::AppHandle,
    pos: PhysicalPosition<i32>,
    size: PhysicalSize<u32>,
) {
    if let Some(window) = app.get_webview_window("main") {
        let panel_w: i32 = 320;
        let x = pos.x + (size.width as i32) / 2 - panel_w / 2;
        let y = pos.y + size.height as i32;
        // 水平方向钳制到主屏幕范围内，避免多屏/边缘越界
        if let Ok(Some(monitor)) = app.primary_monitor() {
            let sw = monitor.size().width as i32;
            let clamped = x.max(8).min((sw - panel_w - 8).max(8));
            let _ = window.set_position(PhysicalPosition::new(clamped, y));
        } else {
            let _ = window.set_position(PhysicalPosition::new(x, y));
        }
        let _ = window.show();
        let _ = window.unminimize();
        let _ = window.set_focus();
    }
}
