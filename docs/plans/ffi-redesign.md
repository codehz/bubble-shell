# FFI 重新设计：从 C FFI 到平台原生 API

## 目标

将当前基于全局状态和线程绑定的 C FFI 重构为 NSH 风格的现代化 API：
- iOS: 惯用的 Swift API
- Android: 惯用的 Kotlin/Java API
- 底层: 基于不透明指针的 C ABI，无全局状态

## 当前问题

### 现有设计（bubble-shell）
```c
// 全局线程局部状态
int bubble_create(const BubbleHost *host);  // 返回 id，绑定到当前线程
int bubble_bind(int id);                     // 切换当前线程的绑定
void bubble_destroy();                       // 销毁当前绑定的 shell
int bubble_frame(float w, float h, ...);     // 隐式使用当前绑定
```

**问题：**
1. 全局状态 (`ENGINES: Mutex<EngineMap>`) + 线程局部 (`CURRENT: thread_local!`)
2. 整数 ID 而非类型安全的指针
3. 错误通过返回码传递，需要单独的 `bubble_last_error()`
4. Swift/Kotlin 层直接调用 C FFI，缺乏类型安全和惯用封装
5. 多实例管理复杂（需要 bind/unbind）

### 参考设计（NSH）
```c
// 不透明指针，显式传递
NshApp *nsh_app_create(CGRect frame, bool trace);
UIView *nsh_app_view(NshApp *app);
char *nsh_app_load_bundle(NshApp *app, const char *path);  // 错误字符串或 NULL
void nsh_app_destroy(NshApp *app);
void nsh_string_free(char *s);
```

**Swift 层：**
```swift
public final class NshAppView: UIView {
    private let app: OpaquePointer
    public init(frame: CGRect = .zero, trace: Bool = false) {
        app = nsh_app_create(frame, trace)
        super.init(frame: frame)
        attach()
    }
    deinit { nsh_app_destroy(app) }
}
```

## 设计决策

基于交互式审查的决策：

1. **目标平台**: iOS (Swift) + Android (Kotlin/Java)
2. **集成目标**: 惯用的平台 API（Swift/Kotlin 包装器隐藏 C 层）
3. **多实例**: 支持一个进程内多个独立 shell（NSH 模式）
4. **破坏性变更**: 可以完全重新设计，无需保持兼容性
5. **错误处理**: NSH 风格 - 函数返回 `char*` 表示错误（NULL = 成功）
6. **线程模型**: 显式文档说明，无运行时强制
7. **视图所有权**: BubbleShell 拥有 UIView/View，通过 getter 返回
8. **API 优先级**: 基础功能 + 文本编辑 + 宿主集成

## 新的 API 设计

### C ABI 层 (`bubble.h`)

```c
#pragma once
#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

// Opaque handle to a shell instance
typedef struct BubbleShell BubbleShell;

// Host callbacks for text measurement and layout
typedef struct {
    void (*measure)(void *ctx, const uint8_t *text, uint32_t text_len, 
                    float font_size, float line_height, uint16_t weight, 
                    float max_width, int has_max, float *out_w, float *out_h);
    int (*layout)(void *ctx, const uint8_t *text, uint32_t text_len, 
                  float width, float font_size, float line_height, 
                  uint16_t weight, uint8_t align, float density, 
                  uint64_t revision, uint8_t *out, uint32_t cap);
    void *ctx;
} BubbleHostCallbacks;

// Error handling
void bubble_string_free(char *s);

// Lifecycle - returns NULL on error with error string
BubbleShell *bubble_create(const BubbleHostCallbacks *host);
void bubble_destroy(BubbleShell *shell);

// Bundle loading
char *bubble_load(BubbleShell *shell, const char *manifest_json, 
                  const uint8_t *entry, uint32_t entry_len);

// Configuration
char *bubble_set_params(BubbleShell *shell, const char *json);
char *bubble_set_storage_dir(BubbleShell *shell, const char *path);

// Rendering - returns 0 on success, -1 on error
int bubble_frame(BubbleShell *shell, float width, float height, 
                 const uint8_t **paint, uint32_t *paint_len);

// Input
int bubble_pointer(BubbleShell *shell, uint32_t action, uint32_t id, 
                   float x, float y, double time_ms, 
                   uint32_t pointer_kind, uint32_t buttons);
int bubble_tick(BubbleShell *shell, double time_ms);
void bubble_scroll(BubbleShell *shell, float x, float y, float dx, float dy);

// Text editing (transactional)
int bubble_edit_snapshot(BubbleShell *shell, uint64_t *session, 
                         uint64_t *revision, uint32_t *anchor, 
                         uint32_t *active, uint32_t *comp_start, 
                         uint32_t *comp_end, uint32_t *composing);
int bubble_edit_commit(BubbleShell *shell, uint64_t session, 
                       const uint8_t *text, uint32_t text_len, 
                       int32_t new_cursor_pos);
int bubble_edit_select(BubbleShell *shell, uint64_t session, 
                       uint32_t anchor, uint32_t active);
// ... more edit functions

// Text field queries
uint32_t bubble_field_at(BubbleShell *shell, float x, float y);
uint32_t bubble_focused(BubbleShell *shell);
uint32_t bubble_field_flags(BubbleShell *shell, uint32_t handle);

// Host integration
char *bubble_send(BubbleShell *shell, const char *name, const char *payload);
int bubble_poll_request(BubbleShell *shell, uint32_t *id, 
                        char *method, uint32_t method_cap, 
                        char *params, uint32_t params_cap);
void bubble_finish_request(BubbleShell *shell, uint32_t id, 
                           int ok, const char *result);

// Fonts
void bubble_register_font(BubbleShell *shell, const char *family, 
                          const uint8_t *data, uint32_t len);
uint32_t bubble_font_epoch(BubbleShell *shell);

// Diagnostics
uint32_t bubble_generation(BubbleShell *shell);
uint32_t bubble_faults(BubbleShell *shell);
const char *bubble_last_error(BubbleShell *shell);

#ifdef __cplusplus
}
#endif
```

### iOS 层

#### `include/module.modulemap`
```
module BubbleShellC {
    header "bubble.h"
    export *
    
    link framework "Foundation"
    link framework "UIKit"
    link framework "CoreGraphics"
    link framework "CoreText"
}
```

#### `BubbleShell.swift`
```swift
import UIKit
internal import BubbleShellC

/// Bubble Shell error
public struct BubbleError: Error, LocalizedError, CustomStringConvertible, Sendable {
    public let message: String
    
    public var description: String { message }
    public var errorDescription: String? { message }
}

private func check(_ error: UnsafeMutablePointer<CChar>?) throws {
    guard let error else { return }
    let message = String(cString: error)
    bubble_string_free(error)
    throw BubbleError(message: message)
}

/// A view running a Bubble Shell app
@MainActor
public final class BubbleView: UIView {
    private let shell: OpaquePointer
    
    /// Creates the view with optional host callbacks
    public init(frame: CGRect = .zero, host: BubbleHostCallbacks? = nil) {
        var callbacks = host
        shell = bubble_create(host != nil ? &callbacks! : nil)
        super.init(frame: frame)
        setupContentView()
    }
    
    public required init?(coder: NSCoder) {
        shell = bubble_create(nil)
        super.init(coder: coder)
        setupContentView()
    }
    
    deinit {
        bubble_destroy(shell)
    }
    
    private func setupContentView() {
        // Setup rendering view as subview
    }
    
    /// Load a bundle
    public func loadBundle(manifest: String, entry: Data) throws {
        try manifest.withCString { manifestPtr in
            try entry.withUnsafeBytes { entryBuffer in
                let entryPtr = entryBuffer.bindMemory(to: UInt8.self).baseAddress!
                try check(bubble_load(shell, manifestPtr, entryPtr, UInt32(entry.count)))
            }
        }
    }
    
    /// Set launch parameters (JSON object)
    public func setParams(_ json: String) throws {
        try json.withCString { ptr in
            try check(bubble_set_params(shell, ptr))
        }
    }
    
    /// Set storage directory
    public func setStorageDirectory(_ path: String?) throws {
        if let path = path {
            try path.withCString { ptr in
                try check(bubble_set_storage_dir(shell, ptr))
            }
        } else {
            try check(bubble_set_storage_dir(shell, nil))
        }
    }
    
    /// Send event to the running app
    public func send(name: String, payload: String) throws {
        try name.withCString { namePtr in
            try payload.withCString { payloadPtr in
                try check(bubble_send(shell, namePtr, payloadPtr))
            }
        }
    }
    
    /// Register a custom font
    public func registerFont(family: String, data: Data) {
        family.withCString { familyPtr in
            data.withUnsafeBytes { buffer in
                let ptr = buffer.bindMemory(to: UInt8.self).baseAddress!
                bubble_register_font(shell, familyPtr, ptr, UInt32(data.count))
            }
        }
    }
    
    // Internal: frame rendering, touch handling, etc.
    func frame(width: Float, height: Float) -> Data? {
        var paintPtr: UnsafePointer<UInt8>?
        var paintLen: UInt32 = 0
        guard bubble_frame(shell, width, height, &paintPtr, &paintLen) == 0,
              let ptr = paintPtr else {
            return nil
        }
        return Data(bytes: ptr, count: Int(paintLen))
    }
}
```

### Android 层

#### `BubbleNative.kt` (JNI)
```kotlin
package dev.bubble.shell

internal object BubbleNative {
    init {
        System.loadLibrary("bubble_ffi")
    }
    
    // Returns handle (long) or 0 on error
    external fun create(): Long
    external fun destroy(handle: Long)
    
    // Returns error string or null on success
    external fun load(handle: Long, manifest: String, entry: ByteArray): String?
    external fun setParams(handle: Long, json: String): String?
    external fun setStorageDir(handle: Long, path: String?): String?
    external fun send(handle: Long, name: String, payload: String): String?
    
    // Frame rendering
    external fun frame(handle: Long, width: Float, height: Float): ByteArray?
    
    // Input
    external fun pointer(handle: Long, action: Int, id: Int, x: Float, y: Float, 
                        timeMs: Double, pointerKind: Int, buttons: Int): Int
    external fun tick(handle: Long, timeMs: Double): Int
    external fun scroll(handle: Long, x: Float, y: Float, dx: Float, dy: Float)
    
    // Text editing
    external fun editSnapshot(handle: Long): LongArray?
    external fun editCommit(handle: Long, session: Long, text: ByteArray, newCursorPos: Int): Int
    external fun editSelect(handle: Long, session: Long, anchor: Int, active: Int): Int
    
    // Queries
    external fun fieldAt(handle: Long, x: Float, y: Float): Int
    external fun focused(handle: Long): Int
    external fun fieldFlags(handle: Long, fieldHandle: Int): Int
    
    // Host integration
    external fun pollRequest(handle: Long): String?
    external fun finishRequest(handle: Long, id: Int, ok: Int, result: String)
    
    // Fonts
    external fun registerFont(handle: Long, family: String, data: ByteArray)
    external fun fontEpoch(handle: Long): Int
    
    // Diagnostics
    external fun generation(handle: Long): Int
    external fun faults(handle: Long): Int
}
```

#### `BubbleView.kt`
```kotlin
package dev.bubble.shell

import android.content.Context
import android.util.AttributeSet
import android.widget.FrameLayout

class BubbleError(message: String) : Exception(message)

class BubbleView @JvmOverloads constructor(
    context: Context,
    attrs: AttributeSet? = null,
    defStyleAttr: Int = 0
) : FrameLayout(context, attrs, defStyleAttr) {
    
    private var handle: Long = 0
    private val hostLock = Any()
    
    init {
        handle = BubbleNative.create()
        if (handle == 0L) {
            throw BubbleError("Failed to create shell")
        }
        setupContentView()
    }
    
    private fun setupContentView() {
        // Setup rendering surface
    }
    
    fun loadBundle(manifest: String, entry: ByteArray) {
        val error = BubbleNative.load(handle, manifest, entry)
        if (error != null) {
            throw BubbleError(error)
        }
    }
    
    fun setParams(json: String) {
        val error = BubbleNative.setParams(handle, json)
        if (error != null) {
            throw BubbleError(error)
        }
    }
    
    fun setStorageDirectory(path: String?) {
        val error = BubbleNative.setStorageDir(handle, path)
        if (error != null) {
            throw BubbleError(error)
        }
    }
    
    fun send(name: String, payload: String) {
        synchronized(hostLock) {
            if (handle == 0L) return
            val error = BubbleNative.send(handle, name, payload)
            if (error != null) {
                throw BubbleError(error)
            }
        }
    }
    
    fun registerFont(family: String, data: ByteArray) {
        BubbleNative.registerFont(handle, family, data)
    }
    
    override fun onDetachedFromWindow() {
        super.onDetachedFromWindow()
        synchronized(hostLock) {
            if (handle != 0L) {
                BubbleNative.destroy(handle)
                handle = 0
            }
        }
    }
    
    // Internal: frame rendering, touch handling
}
```

## 实现步骤

### Phase 1: Core C FFI 重构

**目标文件：** `crates/native/src/lib.rs`, `crates/native/include/bubble.h`

1. **移除全局状态**
   - 删除 `ENGINES: LazyLock<Mutex<EngineMap>>`
   - 删除 `CURRENT: thread_local!<Cell<i32>>`
   - 将 `Shared` 和关联状态封装到不透明的 `BubbleShell` 结构

2. **重写导出函数**
   ```rust
   // 旧
   #[no_mangle]
   pub extern "C" fn bubble_create(host: *const BubbleHost) -> i32 { ... }
   
   // 新
   #[no_mangle]
   pub extern "C" fn bubble_create(host: *const BubbleHostCallbacks) -> *mut BubbleShell {
       let shell = match create_shell_impl(host) {
           Ok(s) => s,
           Err(_) => return std::ptr::null_mut(),
       };
       Box::into_raw(Box::new(shell))
   }
   
   #[no_mangle]
   pub unsafe extern "C" fn bubble_destroy(shell: *mut BubbleShell) {
       if !shell.is_null() {
           drop(Box::from_raw(shell));
       }
   }
   ```

3. **错误处理**
   ```rust
   fn error_string(msg: impl std::fmt::Display) -> *mut c_char {
       CString::new(msg.to_string().replace('\0', " "))
           .unwrap_or_default()
           .into_raw()
   }
   
   #[no_mangle]
   pub unsafe extern "C" fn bubble_string_free(s: *mut c_char) {
       if !s.is_null() {
           drop(CString::from_raw(s));
       }
   }
   ```

4. **更新所有 API 函数**
   - 第一个参数改为 `shell: *mut BubbleShell`
   - 返回 `char*` 的函数用于错误（NULL = 成功）
   - 返回值的函数保持 `int` 或其他类型

### Phase 2: iOS Swift 包装

**目标文件：** `sdk/ios/`

1. **创建 module map**
   - `include/module.modulemap`
   - 引用 `bubble.h`

2. **实现 `BubbleShell.swift`**
   - `BubbleError` 错误类型
   - `BubbleView: UIView` 主类
   - 私有 C 互操作帮助函数
   - 公共 Swift API（throw 错误）

3. **更新 Xcode 项目**
   - 配置 module map 路径
   - 链接新的静态库
   - 更新示例应用

### Phase 3: Android Kotlin/Java 包装

**目标文件：** `sdk/android/bubble/src/main/kotlin/`

1. **JNI 层 (`BubbleNative.kt`)**
   - 使用 `external fun` 声明
   - 返回 `Long` 句柄
   - 返回 `String?` 作为错误

2. **Rust JNI 实现**
   - `Java_dev_bubble_shell_BubbleNative_create`
   - 使用 `jlong` 传递句柄
   - 使用 `jni` crate 的 `JString` 处理字符串

3. **Kotlin 包装 (`BubbleView.kt`)**
   - `BubbleError` 异常类
   - `BubbleView: FrameLayout` 主类
   - 内部句柄管理和生命周期

4. **构建配置**
   - 更新 `build.gradle.kts`
   - 配置 JNI 库路径
   - 更新示例应用

### Phase 4: 测试与文档

1. **单元测试**
   - Rust: `crates/native/tests/` 更新为新 API
   - iOS: XCTest 集成测试
   - Android: JUnit 集成测试

2. **示例应用**
   - iOS: 更新 `sdk/ios/` 示例
   - Android: 更新 `sdk/android/` 示例

3. **文档**
   - API 参考（Rust doc, Swift doc, KDoc）
   - 迁移指南（本文档的一部分）

## 测试策略

### Rust 层测试
```rust
#[test]
fn test_create_destroy() {
    unsafe {
        let shell = bubble_create(std::ptr::null());
        assert!(!shell.is_null());
        bubble_destroy(shell);
    }
}

#[test]
fn test_load_bundle() {
    unsafe {
        let shell = bubble_create(std::ptr::null());
        let manifest = CString::new(r#"{"id":"test","entry":"index.js"}"#).unwrap();
        let entry = b"console.log('test')";
        let error = bubble_load(shell, manifest.as_ptr(), entry.as_ptr(), entry.len() as u32);
        assert!(error.is_null());
        bubble_destroy(shell);
    }
}
```

### iOS 层测试
```swift
func testCreateDestroy() {
    let view = BubbleView(frame: .zero)
    XCTAssertNotNil(view)
}

func testLoadBundle() throws {
    let view = BubbleView(frame: .zero)
    let manifest = #"{"id":"test","entry":"index.js"}"#
    let entry = "console.log('test')".data(using: .utf8)!
    try view.loadBundle(manifest: manifest, entry: entry)
}
```

### Android 层测试
```kotlin
@Test
fun testCreateDestroy() {
    val view = BubbleView(context)
    assertNotNull(view)
}

@Test
fun testLoadBundle() {
    val view = BubbleView(context)
    val manifest = """{"id":"test","entry":"index.js"}"""
    val entry = "console.log('test')".toByteArray()
    view.loadBundle(manifest, entry)
}
```

## 性能考虑

1. **无全局锁竞争**: 每个 shell 独立，无全局 `ENGINES` 互斥锁
2. **线程局部开销消除**: 不再需要 `thread_local!` 查找
3. **指针传递**: 直接传递指针比整数 ID 查找更快
4. **内存布局**: `BubbleShell` 可以优化为更紧凑的布局

## 风险与缓解

| 风险 | 缓解 |
|------|------|
| 破坏现有集成 | 完全重写，已确认可接受 |
| JNI 句柄管理复杂 | 参考 NSH 的成熟模式 |
| Swift module map 配置 | 使用 NSH 相同的结构 |
| 测试覆盖不足 | 每层都有单元测试和集成测试 |
| 文档过期 | 文档与实现同步更新 |

## 未来扩展

1. **Web 平台**: WASM 绑定可以遵循相同模式
2. **桌面平台**: macOS/Linux/Windows 原生包装
3. **组件 API**: 自定义原生组件注册（参考 NSH 的 `nsh_app_register_component`）
4. **滚动监听**: 原生滚动集成（参考 NSH 的 `nsh_app_watch_scroll`）
5. **开发模式**: 热重载和开发服务器连接

## 参考

- NSH 嵌入 API: `nsh/crates/nsh-platform-ios/src/embed.rs`
- NSH Swift 模块: `nsh/crates/nsh-platform-ios/swift/NativeShell.swift`
- NSH Android JNI: `nsh/crates/nsh-platform-android/src/host.rs`
- 当前 bubble-shell FFI: `crates/native/src/lib.rs`
