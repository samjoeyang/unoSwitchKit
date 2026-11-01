// SwitchKit Swift sidecar（M4）
//
// 编译： swiftc -O -framework AppKit -framework ApplicationServices -framework IOBluetooth \
//   main.swift bridge.swift bluetooth.swift -o switchkit-daemon
// 运行： ./switchkit-daemon   （随 Tauri 打包为 sidecar，自动拉起）
//
// 职责（承载纯 Web 无法完成的原生能力）：
//   - 蓝牙设备列表/连接：IOBluetooth；蓝牙开关：blueutil（需 brew install）
//
// 协议：127.0.0.1:9876，换行分隔的 JSON 行（详见 bridge.swift 的协议常量）。
//
// 网络层用「裸 BSD socket」而非 Network 框架：
//   - 不依赖任何 runloop/事件循环泵送，进程用 dispatchMain() 常驻即可；
//   - 绕开本机网络扩展（如 Tailscale）对 Network 框架 bind 的拦截——本机实测
//     NWListener 在 9876 上始终无法就绪，裸 BSD socket 可正常监听。

import Foundation
import AppKit

let port: UInt16 = 9876

/// 创建并绑定 loopback 监听 socket；失败返回 nil。
func makeListener() -> Int32? {
    let fd = socket(AF_INET, SOCK_STREAM, IPPROTO_TCP)
    guard fd >= 0 else {
        FileHandle.standardError.write(
            "sidecar: socket() failed\n".data(using: .utf8)!)
        return nil
    }
    // SO_REUSEADDR：避免上一次退出残留的 TIME_WAIT 直接阻止本次 bind。
    var yes: Int32 = 1
    setsockopt(
        fd, SOL_SOCKET, SO_REUSEADDR, &yes,
        socklen_t(MemoryLayout.size(ofValue: yes)))

    var addr = sockaddr_in()
    addr.sin_len = UInt8(MemoryLayout<sockaddr_in>.size)
    addr.sin_family = sa_family_t(AF_INET)
    addr.sin_port = port.bigEndian
    addr.sin_addr = in_addr(s_addr: inet_addr("127.0.0.1"))

    let bindOk = withUnsafePointer(to: addr) { p in
        p.withMemoryRebound(to: sockaddr.self, capacity: 1) { sa in
            bind(fd, sa, socklen_t(MemoryLayout<sockaddr_in>.size))
        }
    }
    guard bindOk == 0 else {
        FileHandle.standardError.write(
            "sidecar: bind(127.0.0.1:\(port)) failed: \(errno)\n"
                .data(using: .utf8)!)
        close(fd)
        return nil
    }
    guard listen(fd, 16) == 0 else {
        FileHandle.standardError.write(
            "sidecar: listen() failed\n".data(using: .utf8)!)
        close(fd)
        return nil
    }
    return fd
}

guard let listenFd = makeListener() else { exit(1) }

// 接受连接：监听 socket 可读即 accept，交給 Connection 处理。
let acceptSrc = DispatchSource.makeReadSource(
    fileDescriptor: listenFd, queue: .global(qos: .userInitiated))
acceptSrc.setEventHandler {
    var clientAddr = sockaddr_in()
    var len = socklen_t(MemoryLayout<sockaddr_in>.size)
    let connFd = withUnsafeMutablePointer(to: &clientAddr) { p in
        p.withMemoryRebound(to: sockaddr.self, capacity: 1) { sa in
            accept(listenFd, sa, &len)
        }
    }
    if connFd >= 0 {
        Connection(fd: connFd).serve()
    }
}
acceptSrc.resume()
// 进程常驻，故意泄漏监听源
_ = Unmanaged.passRetained(acceptSrc)

FileHandle.standardError.write(
    "sidecar: listening on 127.0.0.1:\(port)\n".data(using: .utf8)!)

// ---- 父进程看门狗 ----
// 本进程是常驻进程（下面的 dispatchMain() 永不返回），若 App 退出时
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
            "sidecar: parent \(parentPid) exited, shutting down\n"
                .data(using: .utf8)!)
        exit(0)
    }
    src.resume()
    // 进程源不持有强引用链，需自己保住；daemon 常驻，故意泄漏
    _ = Unmanaged.passRetained(src)
    FileHandle.standardError.write(
        "sidecar: watching parent pid \(parentPid)\n".data(using: .utf8)!)
}

// 守护进程不依赖任何 UI（菜单栏收纳 menubar.swift 已移除），无需 NSApplication；
// 用 dispatchMain() 常驻（GCD 管理的循环，驱动上面 .global 队列的监听/连接回调），
// 保持进程存活（看门狗也在后台队列）。
dispatchMain()
