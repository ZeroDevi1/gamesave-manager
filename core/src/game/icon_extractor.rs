// game/icon_extractor.rs - 从 exe 程序物理提取高清图标并编码为 Base64 传递给前端
use std::path::Path;
use tauri::AppHandle;

/// 拉起 native 物理选择框让用户定位 exe 文件，并提取高清图标。
///
/// # 核心业务语义：
/// 1. 弹出跨平台物理对话框，限定过滤选择 `.exe` 扩展名。
/// 2. Windows 环境下，通过句柄物理提取 exe 内嵌的高清图标，自动转码为 PNG + Base64。
/// 3. 前端可直接无痛渲染返回的 Data URI 进行实时预览。
///
/// # 返回值：
/// * `Result<Option<serde_json::Value>, String>` - 包含 "base64" (Data URI) 和 "path" (本地物理路径) 的 JSON 对象
pub async fn select_and_extract_exe_icon(_app: AppHandle) -> Result<Option<serde_json::Value>, String> {
    // 调用 RFD 物理对话框选择 exe 文件
    let file = rfd::AsyncFileDialog::new()
        .add_filter("游戏可执行文件", &["exe"])
        .pick_file()
        .await;

    let path = match file {
        Some(f) => f.path().to_path_buf(),
        None => return Ok(None),
    };

    // 提取图标为 Base64 Data URI
    let base64_icon = extract_icon_to_base64(&path)?;

    Ok(Some(serde_json::json!({
        "base64": base64_icon,
        "path": path.to_string_lossy().to_string(),
    })))
}

/// Windows 平台的物理提取底层逻辑实现
#[cfg(target_os = "windows")]
fn extract_icon_to_base64(path: &Path) -> Result<Option<String>, String> {
    use std::os::windows::ffi::OsStrExt;
    use windows::Win32::UI::WindowsAndMessaging::{DestroyIcon, GetIconInfo, PrivateExtractIconsW, HICON};
    use windows::Win32::Graphics::Gdi::{
        CreateCompatibleDC, DeleteDC, DeleteObject, GetDIBits, BITMAPINFO,
        BITMAPINFOHEADER, DIB_RGB_COLORS, HDC,
    };

    let path_str = path.as_os_str();
    let path_wide: Vec<u16> = path_str.encode_wide().collect();
    
    // windows-rs 0.58.0 签名是 `szfilename: &[u16; 260]` 静态引用。
    // 我们必须建立固定 260 长度的缓冲区，把宽字符拷贝进去
    let mut filename = [0u16; 260];
    let len = path_wide.len().min(259);
    filename[..len].copy_from_slice(&path_wide[..len]);

    unsafe {
        let mut hicons = [HICON::default(); 1];
        let mut icon_ids = [0u32; 1];
        
        // 第一优先级：尝试提取 256x256 的极致高清图标
        let mut count = PrivateExtractIconsW(
            &filename,
            0,
            256,
            256,
            Some(&mut hicons),
            Some(icon_ids.as_mut_ptr()),
            1,
        );

        // 第二优先级：若无 256 大图，则提取 128x128 尺寸
        if count == 0 || hicons[0].is_invalid() {
            count = PrivateExtractIconsW(
                &filename,
                0,
                128,
                128,
                Some(&mut hicons),
                Some(icon_ids.as_mut_ptr()),
                1,
            );
        }

        // 第三优先级：若无 128 大图，则退避提取 48x48 尺寸
        if count == 0 || hicons[0].is_invalid() {
            count = PrivateExtractIconsW(
                &filename,
                0,
                48,
                48,
                Some(&mut hicons),
                Some(icon_ids.as_mut_ptr()),
                1,
            );
        }

        // 全面失效则安全返回
        if count == 0 || hicons[0].is_invalid() {
            return Ok(None);
        }

        let hicon = hicons[0];
        let mut icon_info = windows::Win32::UI::WindowsAndMessaging::ICONINFO::default();
        
        // GetIconInfo 返回 Result<()>，is_err() 表明调用失败
        if GetIconInfo(hicon, &mut icon_info).is_err() {
            let _ = DestroyIcon(hicon);
            return Err("提取图标的物理信息失败".to_string());
        }

        // 统一清理闭包，预防 GDI 泄漏
        let cleanup = || {
            let _ = DestroyIcon(hicon);
            if !icon_info.hbmColor.is_invalid() {
                let _ = DeleteObject(icon_info.hbmColor);
            }
            if !icon_info.hbmMask.is_invalid() {
                let _ = DeleteObject(icon_info.hbmMask);
            }
        };

        let hbitmap = icon_info.hbmColor;
        if hbitmap.is_invalid() {
            cleanup();
            return Err("图标不含彩色位图层".to_string());
        }

        // 构造 DC 用于拷贝图形缓冲区
        let hdc = CreateCompatibleDC(HDC::default());
        if hdc.is_invalid() {
            cleanup();
            return Err("创建兼容设备上下文失败".to_string());
        }

        let mut bmi = BITMAPINFO::default();
        bmi.bmiHeader.biSize = std::mem::size_of::<BITMAPINFOHEADER>() as u32;

        // 探测位图基础元数据
        if GetDIBits(hdc, hbitmap, 0, 0, None, &mut bmi, DIB_RGB_COLORS) == 0 {
            let _ = DeleteDC(hdc);
            cleanup();
            return Err("探测位图元数据头部失败".to_string());
        }

        let width = bmi.bmiHeader.biWidth;
        let height = bmi.bmiHeader.biHeight.abs();

        // 强制重构为 32位 RGBA 规范
        bmi.bmiHeader.biBitCount = 32;
        bmi.bmiHeader.biCompression = 0; // BI_RGB
        bmi.bmiHeader.biSizeImage = (width * height * 4) as u32;
        bmi.bmiHeader.biHeight = -height; // 为负数代表自顶向下的 DIB 排布，省去倒置循环

        let mut buf = vec![0u8; (width * height * 4) as usize];

        // 物理抓取像素缓冲（Windows 默认以 BGRA 顺序传回）
        if GetDIBits(
            hdc,
            hbitmap,
            0,
            height as u32,
            Some(buf.as_mut_ptr() as *mut _),
            &mut bmi,
            DIB_RGB_COLORS,
        ) == 0
        {
            let _ = DeleteDC(hdc);
            cleanup();
            return Err("拷贝图形物理像素流失败".to_string());
        }

        // 强制释放所有 Win32 句柄与 GDI 资源，严防泄露
        let _ = DeleteDC(hdc);
        cleanup();

        // 纠正 alpha 通道：如果所有像素的 alpha 皆为 0（表明原图无透明属性但写入了0），则强制填满 255
        let mut has_visible_alpha = false;
        for i in 0..(width * height) as usize {
            if buf[i * 4 + 3] > 0 {
                has_visible_alpha = true;
                break;
            }
        }

        // BGRA 物理重组至标准的 RGBA PNG 排列
        let mut rgba_buf = vec![0u8; (width * height * 4) as usize];
        for i in 0..(width * height) as usize {
            let b = buf[i * 4];
            let g = buf[i * 4 + 1];
            let r = buf[i * 4 + 2];
            let a = if has_visible_alpha { buf[i * 4 + 3] } else { 255 };

            rgba_buf[i * 4] = r;
            rgba_buf[i * 4 + 1] = g;
            rgba_buf[i * 4 + 2] = b;
            rgba_buf[i * 4 + 3] = a;
        }

        // 在内存中使用 png 编码器压缩
        let mut png_bytes = Vec::new();
        {
            let mut encoder = png::Encoder::new(&mut png_bytes, width as u32, height as u32);
            encoder.set_color(png::ColorType::Rgba);
            encoder.set_depth(png::BitDepth::Eight);
            let mut writer = encoder.write_header().map_err(|e| e.to_string())?;
            writer.write_image_data(&rgba_buf).map_err(|e| e.to_string())?;
        }

        // 转换为 Base64
        use base64::Engine;
        let base64_str = base64::engine::general_purpose::STANDARD.encode(&png_bytes);
        Ok(Some(format!("data:image/png;base64,{}", base64_str)))
    }
}

/// 非 Windows 系统的 Mock 实现，避免交叉编译时构建崩盘
#[cfg(not(target_os = "windows"))]
fn extract_icon_to_base64(_path: &Path) -> Result<Option<String>, String> {
    Ok(None)
}
