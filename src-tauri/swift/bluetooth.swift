// SwitchKit sidecar 蓝牙设备管理：基于系统框架 IOBluetooth（非第三方 CLI）。
// 提供：已配对设备枚举 / 一键连接（轮询等待，最长 10s）/ 断开。
// 首次调用会触发系统的「蓝牙使用」授权弹窗，属预期行为。
// 注：现代 SDK 的 IOBluetoothDevice 无 isConnecting 查询，连接中状态由前端 busy 态表达。

import Foundation
import IOBluetooth

/// 已配对设备快照。
struct BTDeviceSnapshot {
    let name: String
    let address: String
    let connected: Bool

    var dict: [String: Any] {
        ["name": name, "address": address, "connected": connected]
    }
}

enum BTManager {
    /// 枚举全部已配对设备。
    static func pairedDevices() -> [[String: Any]] {
        guard let list = IOBluetoothDevice.pairedDevices() as? [IOBluetoothDevice]
        else { return [] }
        return list.map { d in
            BTDeviceSnapshot(
                name: d.nameOrAddress ?? "",
                address: d.addressString ?? "",
                connected: d.isConnected()
            ).dict
        }
    }

    /// 一键连接：openConnection 为异步，轮询等待完成（最长 10s）。
    static func connect(address: String) -> Bool {
        guard let d = Self.find(address) else { return false }
        if d.isConnected() { return true }
        let ret = d.openConnection()
        guard ret == kIOReturnSuccess else { return false }
        for _ in 0..<20 {
            usleep(500_000)
            if d.isConnected() { return true }
        }
        return d.isConnected()
    }

    /// 断开连接。
    static func disconnect(address: String) -> Bool {
        guard let d = Self.find(address) else { return false }
        if !d.isConnected() { return true }
        d.closeConnection()
        return true
    }

    static func find(_ address: String) -> IOBluetoothDevice? {
        guard let list = IOBluetoothDevice.pairedDevices() as? [IOBluetoothDevice]
        else { return nil }
        return list.first { $0.addressString == address }
    }
}
