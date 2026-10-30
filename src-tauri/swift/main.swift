// SwitchKit Swift sidecar（M4）
//
// 编译： swiftc -framework AppKit daemon.swift bridge.swift -o switchkit-sidecar
// 运行： ./switchkit-sidecar   （随 Tauri 打包为 sidecar，自动拉起）
//
// 职责（承载纯 Web 无法完成的原生能力）：
//   - 蓝牙开关：调用 blueutil CLI（第三方，需 brew install blueutil）
//   - 前台应用监听：NSWorkspace.didActivateApplicationNotification
//
// 协议：127.0.0.1:9876，换行分隔的 JSON 行（详见 bridge.swift 的协议常量）。
// 设计说明：MVP 采用 TCP+JSON 行协议以零依赖落地；如需双向推送可平滑替换为
// WebSocket（Rust 侧已用 std::net::TcpStream，改动极小）。

import Foundation
import Network
import AppKit

let port: UInt16 = 9876
let listener = try! NWListener(using: .tcp, on: NWEndpoint.Port(integerLiteral: port))

listener.stateUpdateHandler = { state in
    switch state {
    case .ready:
        print("SwitchKit sidecar listening on 127.0.0.1:\(port)")
    case .failed(let err):
        print("listener failed: \(err)")
    default:
        break
    }
}

listener.newConnectionHandler = { conn in
    conn.start(queue: .global(qos: .userInitiated))
    Connection(conn: conn).serve()
}

// 前台应用变更 -> 通过已连接的控制通道广播（简单起见仅记录日志）
DistributedNotificationCenter.default.addObserver(
    forName: NSWorkspace.didActivateApplicationNotification,
    object: nil,
    queue: .main
) { note in
    if let app = NSWorkspace.shared.frontmostApplication {
        print("frontmost: \(app.localizedName ?? "")")
    }
}

listener.start(queue: .main)
// 辅助功能授权引导（仅首次弹一次系统设置引导）
MenuBarManager.shared.requestTrustOnce()
RunLoop.main.run()
