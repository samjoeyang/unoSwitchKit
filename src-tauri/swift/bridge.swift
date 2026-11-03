// SwitchKit sidecar 桥接层：JSON 行协议编解码 + 原生能力封装。
// 被 main.swift 引用，随其一起编译。
//
// 网络层使用「裸 BSD socket」（recv/send + DispatchSourceRead），不使用 Network 框架：
//   - 不依赖任何 runloop/事件循环泵送，进程用 dispatchMain() 常驻即可；
//   - 绕开本机网络扩展（如 Tailscale）对 Network 框架 bind 的拦截——本机实测
//     NWListener 在 9876 上永远无法就绪，而裸 BSD socket 可正常监听。

import Foundation
import AppKit
import IOBluetooth

// 协议常量
let PROTOCOL_LINE_SEP: UInt8 = 10 // '\n'

/// 单条连接的处理封装：逐行读取 JSON 命令并回写 JSON 响应。
class Connection {
    let fd: Int32
    private var buffer = Data()
    private var source: DispatchSourceRead?
    private var retainToken: Unmanaged<Connection>?

    init(fd: Int32) { self.fd = fd }

    func serve() {
        // 自身保活：连接存续期间持有一个强引用，cancel 时释放。
        retainToken = Unmanaged.passRetained(self)

        // 非阻塞：DispatchSourceRead 仅在可读时回调，但 accept 得到的新 fd 默认阻塞，
        // 设为非阻塞避免偶发情况下的队列阻塞。
        var flags = fcntl(fd, F_GETFL, 0)
        flags |= O_NONBLOCK
        _ = fcntl(fd, F_SETFL, flags)

        let s = DispatchSource.makeReadSource(
            fileDescriptor: fd, queue: .global(qos: .userInitiated))
        source = s
        s.setEventHandler { [weak self] in self?.onReadable() }
        s.setCancelHandler { [weak self] in self?.cleanup() }
        s.resume()
    }

    private func onReadable() {
        var buf = [UInt8](repeating: 0, count: 4096)
        while true {
            let n = buf.withUnsafeMutableBytes { ptr in
                read(fd, ptr.baseAddress!, ptr.count)
            }
            if n > 0 {
                buffer.append(Data(bytes: buf, count: n))
                drainLines()
            } else if n == 0 {
                // 对端关闭
                source?.cancel()
                return
            } else {
                if errno == EAGAIN { return }
                source?.cancel()
                return
            }
        }
    }

    private func drainLines() {
        while let idx = buffer.firstIndex(of: PROTOCOL_LINE_SEP) {
            let line = buffer.subdata(in: 0..<idx)
            if idx + 1 < buffer.count {
                buffer.removeSubrange(0...idx)
            } else {
                buffer.removeAll()
            }
            if let str = String(data: line, encoding: .utf8) {
                process(message: str)
            }
        }
    }

    private func process(message: String) {
        skLog("recv: \(message)")
        guard
            let data = message.data(using: .utf8),
            let obj = try? JSONSerialization.jsonObject(with: data)
                as? [String: Any],
            let cmd = obj["cmd"] as? String
        else {
            skLog("recv: ignored (not a {\"cmd\":...} JSON)")
            return
        }

        skLog("dispatch: \(cmd)")
        let t0 = Date()
        let resp: [String: Any]
        switch cmd {
        case "ping":
            resp = ["ok": true]
        case "bluetooth:status":
            // 只读缓存（0ms），绝不在此同步调 IOBluetooth（冷启动会挂起 ~14.5s）
            var d: [String: Any] = ["cmd": "bluetooth:status"]
            btCacheQ.sync {
                d["power"] = btPowerCache as Any
                d["warming"] = !btWarmed
            }
            resp = d
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
            // 只读缓存（0ms）；未就绪返回 warming:true，前端显示「初始化中」
            var d: [String: Any] = ["cmd": cmd]
            btCacheQ.sync {
                d["devices"] = btDevicesCache
                d["warming"] = !btWarmed
            }
            resp = d
        case "bt:connect":
            // 连接为异步 + 10s 轮询，同步阻塞本连接直至有结论
            let ok = BTManager.connect(address: obj["address"] as? String ?? "")
            resp = ["cmd": cmd, "ok": ok]
        case "bt:disconnect":
            let ok = BTManager.disconnect(address: obj["address"] as? String ?? "")
            resp = ["cmd": cmd, "ok": ok]
        default:
            resp = ["error": "unknown command: \(cmd)"]
        }
        let ms = Int(-t0.timeIntervalSinceNow * 1000)
        skLog("respond: \(cmd) took \(ms)ms -> \(resp)")
        send(dict: resp)
    }

    private func send(dict: [String: Any]) {
        guard
            let data = try? JSONSerialization.data(withJSONObject: dict),
            let str = String(data: data, encoding: .utf8)
        else { return }
        let bytes = [UInt8](str.utf8) + [PROTOCOL_LINE_SEP]
        var off = 0
        while off < bytes.count {
            let w = bytes.withUnsafeBytes { ptr in
                write(fd, ptr.baseAddress!.advanced(by: off), bytes.count - off)
            }
            if w < 0 { return }
            off += Int(w)
        }
    }

    private func cleanup() {
        close(fd)
        source = nil
        retainToken?.release()
        retainToken = nil
    }

    deinit {
        // 兜底：若 source 未触发 cancel，这里确保 fd 关闭。
        close(fd)
    }
}

// ---------------- 蓝牙结果缓存 ----------------
// 消除 macOS 27 上 IOBluetooth 首次调用 ~14.5s 冷启动导致的探测超时。
// 冷启动是「一次性」成本：首个 IOBluetooth 调用耗时 ~14.5s（bluetoothd XPC 预热），
// 预热后全部 0ms。策略：守护启动即后台预热填充缓存；请求路径只读缓存（0ms，绝不在
// 同步路径碰 IOBluetooth），未就绪返回 warming:true，前端据此显示「初始化中」而非误判。
// 后台定时刷新保持缓存新鲜（蓝牙开关/配对变化后前端下次轮询即见）。
private let btCacheQ = DispatchQueue(label: "bt.cache")       // 保护缓存读写
private let btRefreshQ = DispatchQueue(label: "bt.refresh")   // 串行执行刷新，避免并发调 IOBluetooth
private var btPowerCache: Bool? = nil
private var btDevicesCache: [[String: Any]] = []
private var btWarmed = false

private func btRefreshCache() {
    // 仅在后台队列调用（预热 / 定时），不在请求路径
    let power = bluetoothPower()
    let devices = BTManager.pairedDevices()
    btCacheQ.sync {
        btPowerCache = power
        btDevicesCache = devices
        btWarmed = true
    }
    skLog("bt cache refreshed: power=\(String(describing: power)) devices=\(devices.count)")
}

/// 启动后台预热 + 定时刷新。守护常驻，定时器故意泄漏。
func btKickoff() {
    skLog("prewarm: starting bluetoothd warmup (background)")
    let t0 = Date()
    btRefreshQ.async {
        btRefreshCache()
        let ms = Int(-t0.timeIntervalSinceNow * 1000)
        skLog("prewarm: bluetoothd ready in \(ms)ms")
    }
    let timer = DispatchSource.makeTimerSource(queue: btRefreshQ)
    timer.schedule(deadline: .now() + 10, repeating: 10)
    timer.setEventHandler { btRefreshCache() }
    timer.resume()
    _ = Unmanaged.passRetained(timer)
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
