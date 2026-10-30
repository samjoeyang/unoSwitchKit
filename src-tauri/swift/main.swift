// SwitchKit Swift sidecar（M4）
//
// 编译： swiftc -O -framework AppKit -framework ApplicationServices -framework Network -framework IOBluetooth \
//   main.swift bridge.swift menubar.swift bluetooth.swift -o switchkit-daemon
// 运行： ./switchkit-daemon   （随 Tauri 打包为 sidecar，自动拉起）
//
// 职责（承载纯 Web 无法完成的原生能力）：
//   - 蓝牙设备列表/连接：IOBluetooth；蓝牙开关：blueutil（需 brew install）
//
// 协议：127.0.0.1:9876，换行分隔的 JSON 行（详见 bridge.swift 的协议常量）。

import Foundation
import Network
import AppKit

let port: UInt16 = 9876
// 端口被旧实例退出中的 TIME_WAIT 短暂占用时等待重试，避免直接崩溃
// （此前 `try! NWListener` 一旦 bind 失败守护进程立刻退出且无人拉起）。
var listener: NWListener? = nil
for attempt in 0..<40 {
    if let l = try? NWListener(using: .tcp, on: NWEndpoint.Port(integerLiteral: port)) {
        listener = l
        if attempt > 0 {
            FileHandle.standardError.write(
                "sidecar: port retry succeeded after \(attempt) attempts\n"
                    .data(using: .utf8)!)
        }
        break
    }
    FileHandle.standardError.write(
        "sidecar: port \(port) busy, retrying (\(attempt + 1))\n"
            .data(using: .utf8)!)
    usleep(500_000)
}
guard let listener = listener else {
    FileHandle.standardError.write("sidecar: cannot bind port \(port), exiting\n"
        .data(using: .utf8)!)
    exit(1)
}

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

listener.start(queue: .main)
// 先初始化 NSApplication（NSStatusItem 依赖窗口服务器连接，顺序不能反）
let app = NSApplication.shared
app.setActivationPolicy(.accessory)
app.run()
