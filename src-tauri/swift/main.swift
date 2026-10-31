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

// ---- 父进程看门狗 ----
// 本进程是 AppKit 常驻进程（下面的 NSApplication.run() 永不返回），若 App 退出时
// 不主动回收，它会一直持有 SwitchKit.app/Contents/MacOS/switchkit-daemon 的文件映射，
// 导致 Finder 覆盖安装时报「应用程序正在运行」。App 正常退出时由 Rust 侧 kill；
// 这里兜住 App 崩溃 / kill -9 / 终端关闭等异常退出的情况。
// 父 PID 由 App 通过 --parent-pid 传入；缺省取 getppid()。父为 launchd(1) 时不启用。
var parentPid: pid_t = getppid()
if let i = CommandLine.arguments.firstIndex(of: "--parent-pid"),
   i + 1 < CommandLine.arguments.count,
   let v = pid_t(CommandLine.arguments[i + 1]) {
    parentPid = v
}
if parentPid > 1 {
    // 用内核的进程退出事件而不是轮询 kill(pid,0)：父进程被杀后若尚未被 reap
    // 会成为僵尸，kill(pid,0) 依旧返回成功，轮询式看门狗会漏判。
    // NOTE_EXIT 在进程终止的瞬间触发，与是否僵尸无关。
    let src = DispatchSource.makeProcessSource(
        identifier: parentPid, eventMask: .exit, queue: .global())
    src.setEventHandler {
        FileHandle.standardError.write(
            "sidecar: parent \(parentPid) exited, shutting down\n".data(using: .utf8)!)
        exit(0)
    }
    src.resume()
    // 进程源不持有强引用链，需自己保住；daemon 常驻，故意泄漏
    _ = Unmanaged.passRetained(src)
    FileHandle.standardError.write(
        "sidecar: watching parent pid \(parentPid)\n".data(using: .utf8)!)
}

// 先初始化 NSApplication（NSStatusItem 依赖窗口服务器连接，顺序不能反）
let app = NSApplication.shared
app.setActivationPolicy(.accessory)
app.run()
