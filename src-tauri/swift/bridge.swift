// SwitchKit sidecar 桥接层：JSON 行协议编解码 + 原生能力封装。
// 被 daemon.swift 引用，随其一起编译。

import Foundation
import AppKit
import Network
import IOBluetooth

// 协议常量
let PROTOCOL_LINE_SEP: UInt8 = 10 // '\n'

/// 单条连接的处理封装：逐行读取 JSON 命令并回写 JSON 响应。
class Connection {
    let conn: NWConnection
    private var buffer = Data()

    init(conn: NWConnection) { self.conn = conn }

    func serve() { receive() }

    private func receive() {
        conn.receive(minimumIncompleteLength: 1, maximumLength: 4096) {
            data,
            _,
            _,
            error in
            if let data = data, !data.isEmpty {
                self.buffer.append(data)
                while let idx = self.buffer.firstIndex(of: PROTOCOL_LINE_SEP) {
                    let line = self.buffer.subdata(in: 0..<idx)
                    if idx + 1 < self.buffer.count {
                        self.buffer.removeSubrange(0...idx)
                    } else {
                        self.buffer.removeAll()
                    }
                    if let str = String(data: line, encoding: .utf8) {
                        self.process(message: str)
                    }
                }
            }
            if error == nil {
                self.receive()
            } else {
                self.conn.cancel()
            }
        }
    }

    private func process(message: String) {
        guard
            let data = message.data(using: .utf8),
            let obj = try? JSONSerialization.jsonObject(with: data)
                as? [String: Any],
            let cmd = obj["cmd"] as? String
        else { return }

        let resp: [String: Any]
        switch cmd {
        case "ping":
            resp = ["ok": true]
        case "bluetooth:status":
            resp = ["cmd": "bluetooth:status", "power": bluetoothPower() as Any]
        case "bluetooth:toggle":
            let on = !(bluetoothPower() ?? false)
            _ = shell("blueutil", ["--power", on ? "on" : "off"])
            resp = ["cmd": "bluetooth:toggle", "power": bluetoothPower() as Any]
        case "frontmostApp":
            resp = [
                "cmd": "frontmostApp",
                "app": NSWorkspace.shared.frontmostApplication?
                    .localizedName ?? "",
            ]
        case "bt:devices":
            resp = ["cmd": cmd, "devices": BTManager.pairedDevices()]
        case "bt:connect":
            // 连接为异步 + 10s 轮询，同步阻塞本连接直至有结论
            let ok = BTManager.connect(address: obj["address"] as? String ?? "")
            resp = ["cmd": cmd, "ok": ok]
        case "bt:disconnect":
            let ok = BTManager.disconnect(address: obj["address"] as? String ?? "")
            resp = ["cmd": cmd, "ok": ok]
        case "menubar:list":
            let items = MenuBarManager.shared.listItems()
            resp = [
                "cmd": cmd,
                "axTrusted": MenuBarManager.shared.isTrusted,
                "collapsed": HideBarController.shared.isCollapsed,
                "items": items.map {
                    [
                        "id": $0.id, "owner": $0.owner, "title": $0.title,
                        "x": $0.x, "y": $0.y, "visible": $0.visible,
                    ] as [String: Any]
                },
            ]
        case "hidebar:state":
            resp = [
                "cmd": cmd,
                "collapsed": HideBarController.shared.isCollapsed,
                "freshStart": HideBarController.shared.consumeFreshStart,
            ]
        case "hidebar:set":
            let collapsed = obj["collapsed"] as? Bool ?? false
            HideBarController.shared.setCollapsed(collapsed)
            resp = [
                "cmd": cmd, "ok": true,
                "collapsed": HideBarController.shared.isCollapsed,
            ]
        case "hidebar:autoplace":
            let excludeIds = obj["excludeIds"] as? [String] ?? []
            let excludeOwners = obj["excludeOwners"] as? [String] ?? []
            let ok = HideBarController.shared.autoplace(
                excludeIds: excludeIds, excludeOwners: excludeOwners)
            resp = ["cmd": cmd, "ok": ok]
        default:
            resp = ["error": "unknown command: \(cmd)"]
        }
        send(dict: resp)
    }

    private func send(dict: [String: Any]) {
        guard
            let data = try? JSONSerialization.data(withJSONObject: dict),
            let str = String(data: data, encoding: .utf8)
        else { return }
        let payload = (str + "\n").data(using: .utf8)!
        conn.send(content: payload, completion: .contentProcessed { _ in })
    }
}

// ---------------- 原生能力封装 ----------------

func bluetoothPower() -> Bool? {
    // 优先系统框架 IOBluetoothHostController（公开 API，无外部依赖）
    if let ctrl = IOBluetoothHostController.default() {
        return ctrl.powerState == kBluetoothHCIPowerStateON
    }
    // 回退：blueutil CLI（第三方，需 brew install blueutil）
    let out = shell("blueutil", ["--power"]).trimmingCharacters(
        in: .whitespacesAndNewlines)
    return Int(out).map { $0 == 1 }
}

/// 同步执行外部命令并捕获 stdout；命令不存在或启动失败时立即返回空串（不挂起）。
@discardableResult
func shell(_ launchPath: String, _ args: [String]) -> String {
    let proc = Process()
    proc.executableURL = URL(fileURLWithPath: launchPath)
    proc.arguments = args
    let pipe = Pipe()
    proc.standardOutput = pipe
    proc.standardError = pipe
    do {
        try proc.run()
    } catch {
        return "" // 如 blueutil 未安装：run 抛异常，waitUntilExit 会永久挂起
    }
    proc.waitUntilExit()
    let data = pipe.fileHandleForReading.readDataToEndOfFile()
    return String(data: data, encoding: .utf8) ?? ""
}
