// SwitchKit sidecar —— 菜单栏图标管理（Bartender/Ice 类功能的 MVP）
//
// 原理：通过辅助功能（Accessibility）API 读取各进程的 AXExtrasMenuBar
// （即该 App 拥有的状态项），用 AXPosition 把不点亮的项移出屏幕左缘实现
// "收纳"，点亮的项移回记录的原位置。策略由 Rust 侧推送（menubar:apply），
// 守护进程每 3 秒巡检一次，保证新出现的图标默认被收纳。
//
// 权限：需要 系统设置 -> 隐私与安全性 -> 辅助功能 授权（首次启动弹窗引导）。

import Foundation
import AppKit
import ApplicationServices

/// 一条菜单栏状态项的快照。
struct MenuBarItemSnapshot {
    let id: String
    let owner: String
    let title: String
    let x: Double
    let y: Double
    let width: Double
    let visible: Bool
}

final class MenuBarManager {
    static let shared = MenuBarManager()
    private init() {}

    private var ownPid: pid_t = 0
    /// 允许显示在菜单栏上的项 id 集合（"点亮"的白名单）。
    private var visibleIds: Set<String> = []
    /// 各项的原始位置（隐藏前记录，恢复时用）。
    private var positions: [String: [Double]] = [:]
    private var managed = true
    private var timer: Timer?
    /// 启动时是否已发起过辅助功能授权引导。
    private var prompted = false

    var isTrusted: Bool { AXIsProcessTrusted() }

    /// 首次运行时向用户发起辅助功能授权弹窗（仅一次）。
    func requestTrustOnce() {
        guard !prompted else { return }
        prompted = true
        let opts = [kAXTrustedCheckOptionPrompt.takeUnretainedValue() as String: true]
            as CFDictionary
        _ = AXIsProcessTrustedWithOptions(opts)
    }

    /// Rust 侧推送策略：白名单 + 已知原始位置 + 总开关。
    func configure(
        ownPid: pid_t, managed: Bool, visibleIds: [String],
        positions: [String: [Double]]
    ) {
        self.ownPid = ownPid
        self.managed = managed
        self.visibleIds = Set(visibleIds)
        // 以 Rust 持久化的位置为准（守护进程重启后可恢复），本地新增的保留
        for (k, v) in positions { self.positions[k] = v }
        enforce()
        if timer == nil {
            timer = Timer.scheduledTimer(withTimeInterval: 3, repeats: true) { _ in
                DispatchQueue.global(qos: .utility).async {
                    MenuBarManager.shared.enforce()
                }
            }
        }
        if !managed { timer?.invalidate(); timer = nil }
    }

    /// 枚举全部状态项快照（不含本进程）。
    func listItems() -> [MenuBarItemSnapshot] {
        enumerate().items
    }

    /// 巡检：未点亮的隐藏、点亮但被系统重排的恢复。
    func enforce() {
        guard isTrusted, managed else { return }
        let result = enumerate()
        for (info, element) in result.elements {
            if visibleIds.contains(info.id) {
                if !info.visible, let p = positions[info.id], p.count == 2 {
                    _ = setElementPosition(
                        element, CGPoint(x: p[0], y: p[1]))
                }
            } else if info.visible {
                // 隐藏前记录原始位置（Rust 侧会持久化）
                if positions[info.id] == nil {
                    positions[info.id] = [info.x, info.y]
                }
                _ = setElementPosition(
                    element, CGPoint(x: -info.width - 240, y: info.y))
            }
        }
    }

    // MARK: - AX 枚举

    private typealias ItemElement = (snapshot: MenuBarItemSnapshot, element: AXUIElement)

    private struct EnumResult {
        var items: [MenuBarItemSnapshot] = []
        var elements: [ItemElement] = []
    }

    private func enumerate() -> EnumResult {
        var result = EnumResult()
        guard isTrusted else { return result }
        let screenW = NSScreen.main?.frame.maxX ?? 1440
        for app in NSWorkspace.shared.runningApplications {
            let pid = app.processIdentifier
            if pid == ownPid || pid == Int(ProcessInfo.processInfo.processIdentifier) {
                continue
            }
            let axApp = AXUIElementCreateApplication(pid)
            var extras: CFTypeRef?
            guard
                AXUIElementCopyAttributeValue(
                    axApp, kAXExtrasMenuBarAttribute as CFString, &extras) == .success,
                let extrasBar = extras
            else { continue }
            var childrenRef: CFTypeRef?
            guard
                AXUIElementCopyAttributeValue(
                    extrasBar as! AXUIElement, kAXChildrenAttribute as CFString,
                    &childrenRef) == .success,
                let children = childrenRef as? [AXUIElement]
            else { continue }

            let ownerName =
                app.localizedName ?? app.bundleIdentifier ?? "未知应用"
            let ownerId = app.bundleIdentifier ?? ownerName
            for child in children {
                let title =
                    axString(child, kAXTitleAttribute)
                    ?? axString(child, kAXDescriptionAttribute) ?? ""
                var pos = CGPoint.zero
                var size = CGSize.zero
                _ = axPoint(child, kAXPositionAttribute, &pos)
                _ = axSize(child, kAXSizeAttribute, &size)
                let id = "\(ownerId)|\(title)"
                // 移出屏幕左缘的视为已隐藏（保留少量容差）
                let visible = pos.x > -100 && pos.x < screenW + 100
                let snapshot = MenuBarItemSnapshot(
                    id: id, owner: ownerName, title: title,
                    x: Double(pos.x), y: Double(pos.y), width: Double(size.width),
                    visible: visible)
                result.items.append(snapshot)
                result.elements.append((snapshot, child))
            }
        }
        return result
    }

    // MARK: - AX 基础工具

    private func axString(_ el: AXUIElement, _ attr: String) -> String? {
        var v: CFTypeRef?
        guard
            AXUIElementCopyAttributeValue(el, attr as CFString, &v) == .success,
            let s = v as? String
        else { return nil }
        return s
    }

    private func axPoint(_ el: AXUIElement, _ attr: String, _ out: inout CGPoint)
        -> Bool
    {
        var v: CFTypeRef?
        guard
            AXUIElementCopyAttributeValue(el, attr as CFString, &v) == .success,
            let val = v as! AXValue?
        else { return false }
        return AXValueGetValue(val, .cgPoint, &out)
    }

    private func axSize(_ el: AXUIElement, _ attr: String, _ out: inout CGSize)
        -> Bool
    {
        var v: CFTypeRef?
        guard
            AXUIElementCopyAttributeValue(el, attr as CFString, &v) == .success,
            let val = v as! AXValue?
        else { return false }
        return AXValueGetValue(val, .cgSize, &out)
    }

    private func setElementPosition(_ el: AXUIElement, _ p: CGPoint) -> Bool {
        var point = p
        guard let val = AXValueCreate(.cgPoint, &point) else { return false }
        return
            AXUIElementSetAttributeValue(el, kAXPositionAttribute as CFString, val)
            == .success
    }
}
