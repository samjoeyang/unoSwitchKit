// SwitchKit sidecar —— 菜单栏图标收纳（Hidden Bar 同款「分隔条撑开」方案）
//
// 原理（实测 macOS 12 上 AX 无法移动其它进程的状态项，一律返回
// kAXErrorFailure/-25200，Bartender 式逐项移动此路不通）：
//   守护进程在菜单栏放置两个自己的状态项：
//     · 箭头（可点击，展开/收起切换；⌘ 拖动可移动边界）
//     · 分隔条（边界载体，收起时长度撑到 ~2 倍屏宽，把分界线左侧的图标
//       全部挤出屏幕左缘；展开时缩回 20pt，图标自动回流）
//   「点亮」某个图标 = 按住 ⌘ 把它拖到箭头右侧（常驻显示）。
//   首次使用时守护进程会把分隔条+箭头自动摆到全部第三方图标右侧（默认
//   全收纳、保留 SwitchKit 自己的托盘），摆不动时降级为用户手动 ⌘ 拖动。
//
// 权限：AX 枚举（只读）仍需 辅助功能 授权；收纳动作本身不依赖 AX。

import Foundation
import AppKit
import ApplicationServices

/// 一条菜单栏状态项的快照（只读展示用）。
struct MenuBarItemSnapshot {
    let id: String
    let owner: String
    let title: String
    let x: Double
    let y: Double
    let width: Double
    let visible: Bool
}

/// AX 只读枚举（供面板展示当前各图标状态）。
final class MenuBarManager {
    static let shared = MenuBarManager()
    private init() {}

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

    /// 枚举全部状态项快照（不含本守护进程）。
    func listItems() -> [MenuBarItemSnapshot] {
        enumerate().items
    }

    // MARK: - AX 枚举

    private func enumerate() -> EnumResult {
        var result = EnumResult()
        guard isTrusted else { return result }
        let ownPid = Int(ProcessInfo.processInfo.processIdentifier)
        let screenW = NSScreen.main?.frame.maxX ?? 1440
        for app in NSWorkspace.shared.runningApplications {
            let pid = app.processIdentifier
            if pid == ownPid { continue }
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
                // 移出屏幕左缘的视为已收纳（保留少量容差）
                let visible = pos.x > -100 && pos.x < screenW + 100
                let snapshot = MenuBarItemSnapshot(
                    id: id, owner: ownerName, title: title,
                    x: Double(pos.x), y: Double(pos.y), width: Double(size.width),
                    visible: visible)
                result.items.append(snapshot)
            }
        }
        return result
    }

    private struct EnumResult {
        var items: [MenuBarItemSnapshot] = []
    }

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
}

/// 菜单栏收纳控制器：箭头 + 分隔条（Hidden Bar 方案）。
final class HideBarController: NSObject {
    static let shared = HideBarController()
    private override init() { super.init() }

    private var arrowItem: NSStatusItem?
    private var separatorItem: NSStatusItem?
    private var started = false
    /// 守护进程本次启动后是否已处理过首连（供 Rust 恢复状态用）。
    private var greeted = false

    var isCollapsed: Bool {
        guard let sep = separatorItem else { return false }
        return sep.length > 20
    }

    /// 是否为本守护进程启动后的第一次查询。
    var consumeFreshStart: Bool {
        if greeted { return false }
        greeted = true
        return true
    }

    /// 收起时的分隔条长度：菜单栏在每个显示器上都有副本，
    /// 需覆盖最宽的屏；macOS 对 NSStatusItem.length 有 10000pt 硬上限。
    private var collapsedLength: CGFloat {
        let screenW = NSScreen.screens.map { $0.frame.width }.max() ?? 1728
        return max(500, min(screenW * 2, 10_000))
    }

    func start() {
        guard !started else { return }
        started = true
        // 创建顺序决定位置：后创建的排在更左侧。
        // 先箭头、后分隔条 => 布局（从右往左）：箭头、分隔条、其它图标。
        let arrow = NSStatusBar.system.statusItem(withLength: NSStatusItem.variableLength)
        arrow.autosaveName = "switchkit-arrow"
        if let button = arrow.button {
            button.image = Self.chevronImage(right: false)
            button.target = self
            button.action = #selector(toggleClicked)
        }
        let separator = NSStatusBar.system.statusItem(withLength: 20)
        separator.autosaveName = "switchkit-separator"
        if let button = separator.button {
            button.image = Self.lineImage()
            button.target = self
            button.action = #selector(toggleClicked)
        }
        arrowItem = arrow
        separatorItem = separator
        NotificationCenter.default.addObserver(
            self, selector: #selector(screenChanged),
            name: NSApplication.didChangeScreenParametersNotification, object: nil)
    }

    @objc private func screenChanged() {
        // 显示器热插拔后菜单栏会重排，收起状态下需要重撑分隔条
        if isCollapsed { separatorItem?.length = collapsedLength }
    }

    @objc private func toggleClicked() {
        setCollapsed(!isCollapsed)
    }

    func setCollapsed(_ collapsed: Bool) {
        guard let sep = separatorItem else { return }
        sep.length = collapsed ? collapsedLength : 20
        arrowItem?.button?.image = Self.chevronImage(right: collapsed)
    }

    // MARK: - 首次自动摆放

    /// 把分隔条与箭头自动摆到第三方图标右侧（排除 excludeIds 前缀 /
    /// excludeOwners 子串，如 SwitchKit 自己的托盘）。
    /// 成功返回 true；AX 自移动失败返回 false（降级为用户手动 ⌘ 拖动）。
    @discardableResult
    func autoplace(excludeIds: [String], excludeOwners: [String]) -> Bool {
        guard let sep = separatorItem, let arrow = arrowItem else { return false }
        let items = MenuBarManager.shared.listItems()
        let target = items
            .filter { it in
                let idExcluded = excludeIds.contains { it.id.hasPrefix($0) }
                let ownerExcluded =
                    excludeOwners.contains { it.owner.contains($0) }
                return it.visible && !idExcluded && !ownerExcluded
            }
            .map { $0.x + $0.width }
            .max() ?? 0
        // 定位自己的两个状态项（分隔条创建序在后 => 位于箭头左侧）
        guard let own = ownElements() else {
            log("autoplace: own elements not found")
            return false
        }
        let y = items.first?.y ?? 1
        let sepW = Double(sep.length)
        // 途径一：进程内 AX 移动（实测多数环境返回 -25200）
        var e1 = setOwnPosition(own.sep, CGPoint(x: target + 4, y: y))
        var e2 = setOwnPosition(
            own.arrow, CGPoint(x: target + 4 + sepW + 4, y: y))
        log("autoplace: target=\(target) ax sep=\(e1.rawValue) arrow=\(e2.rawValue)")
        if e1 != .success || e2 != .success {
            // 途径二：菜单栏图标顺序由状态项窗口的层级顺序决定（⌘ 拖动的
            // 底层机制），反复 orderFront 把自己顶到最前（最右）。
            if let sepWin = sep.button?.window, let arrowWin = arrow.button?.window {
                for _ in 0..<3 {
                    sepWin.orderFrontRegardless()
                    arrowWin.orderFrontRegardless()
                }
                // 回读实际位置验证
                if let re = ownElements() {
                    var nx: CGFloat = 0
                    _ = axPointOf(re.sep, kAXPositionAttribute, &nx)
                    log("autoplace: window order, sep now x=\(nx)")
                    if Double(nx) >= target - 20 {
                        e1 = .success
                        e2 = .success
                    }
                }
            }
        }
        return e1 == .success && e2 == .success
    }

    private func log(_ line: String) {
        let text = "[hidebar] \(line)\n"
        FileHandle.standardError.write(text.data(using: .utf8)!)
        let path = NSHomeDirectory() + "/Library/Logs/switchkit-daemon.log"
        if let fh = FileHandle(forWritingAtPath: path) {
            fh.seekToEndOfFile()
            fh.write(text.data(using: .utf8)!)
            try? fh.close()
        } else {
            try? text.write(toFile: path, atomically: true, encoding: .utf8)
        }
    }

    /// 枚举本进程 AXExtrasMenuBar 的子项，按 x 升序返回
    /// （左 = 分隔条，右 = 箭头）。进程内 AX 调用不需要辅助功能授权。
    private func ownElements() -> (sep: AXUIElement, arrow: AXUIElement)? {
        let ownPid = ProcessInfo.processInfo.processIdentifier
        let axApp = AXUIElementCreateApplication(ownPid)
        var extras: CFTypeRef?
        guard
            AXUIElementCopyAttributeValue(
                axApp, kAXExtrasMenuBarAttribute as CFString, &extras) == .success,
            let extrasBar = extras
        else {
            log("ownElements: no extras menubar")
            return nil
        }
        var childrenRef: CFTypeRef?
        guard
            AXUIElementCopyAttributeValue(
                extrasBar as! AXUIElement, kAXChildrenAttribute as CFString,
                &childrenRef) == .success,
            let children = childrenRef as? [AXUIElement]
        else {
            log("ownElements: no children")
            return nil
        }
        log("ownElements: children=\(children.count)")
        guard children.count == 2 else { return nil }
        var xs: [CGFloat] = [0, 0]
        for (i, child) in children.enumerated() {
            _ = axPointOf(child, kAXPositionAttribute, &xs[i])
        }
        let sorted = zip(children, xs).sorted { $0.1 < $1.1 }
        return (sorted[0].0, sorted[1].0)
    }

    private func setOwnPosition(_ el: AXUIElement, _ p: CGPoint) -> AXError {
        var point = p
        guard let val = AXValueCreate(.cgPoint, &point) else { return .failure }
        return AXUIElementSetAttributeValue(
            el, kAXPositionAttribute as CFString, val)
    }

    private func axPointOf(_ el: AXUIElement, _ attr: String, _ out: inout CGFloat)
        -> Bool
    {
        var v: CFTypeRef?
        var pos = CGPoint.zero
        guard
            AXUIElementCopyAttributeValue(el, attr as CFString, &v) == .success,
            let val = v as! AXValue?
        else { return false }
        guard AXValueGetValue(val, .cgPoint, &pos) else { return false }
        out = pos.x
        return true
    }

    // MARK: - 图标绘制（模板图，自动适配菜单栏深浅色）

    private static func chevronImage(right: Bool) -> NSImage? {
        let name = right ? "chevron.right" : "chevron.left"
        let img = NSImage(systemSymbolName: name, accessibilityDescription: nil)
        img?.isTemplate = true
        return img
    }

    /// 细竖线分隔符（Hidden Bar 的 ic_line 同款，程序化绘制免资源文件）。
    private static func lineImage() -> NSImage {
        let size = NSSize(width: 3, height: 16)
        let img = NSImage(size: size)
        img.lockFocus()
        NSColor.black.setFill()
        NSBezierPath(
            roundedRect: NSRect(x: 0.5, y: 1, width: 2, height: 14), xRadius: 1,
            yRadius: 1
        ).fill()
        img.unlockFocus()
        img.isTemplate = true
        return img
    }
}
