package game

import (
	"bytes"
	"encoding/base64"
	"errors"
	"image"
	"image/png"
	"syscall"
	"unsafe"
)

var (
	user32                 = syscall.NewLazyDLL("user32.dll")
	gdi32                  = syscall.NewLazyDLL("gdi32.dll")
	procPrivateExtractIcon = user32.NewProc("PrivateExtractIconsW")
	procGetIconInfo        = user32.NewProc("GetIconInfo")
	procDestroyIcon        = user32.NewProc("DestroyIcon")
	procCreateCompatibleDC = gdi32.NewProc("CreateCompatibleDC")
	procDeleteDC           = gdi32.NewProc("DeleteDC")
	procDeleteObject       = gdi32.NewProc("DeleteObject")
	procGetDIBits          = gdi32.NewProc("GetDIBits")
)

type iconInfo struct {
	FIcon    int32
	XHotspot uint32
	YHotspot uint32
	HbmMask  uintptr
	HbmColor uintptr
}

type bitmapInfoHeader struct {
	Size          uint32
	Width         int32
	Height        int32
	Planes        uint16
	BitCount      uint16
	Compression   uint32
	SizeImage     uint32
	XPelsPerMeter int32
	YPelsPerMeter int32
	ClrUsed       uint32
	ClrImportant  uint32
}

// bitmapInfo 在头之后预留颜色表空间：探测元数据时 GetDIBits 可能写入调色板或 BI_BITFIELDS 掩码。
type bitmapInfo struct {
	Header bitmapInfoHeader
	Colors [256]uint32
}

// ExtractIconDataURI 提取 exe 内嵌图标（依次尝试 256、128、48 像素），返回 PNG data URI；
// exe 没有图标时返回 nil。
func ExtractIconDataURI(exePath string) (*string, error) {
	name, err := syscall.UTF16PtrFromString(exePath)
	if err != nil {
		return nil, err
	}
	var hicon uintptr
	for _, size := range []uintptr{256, 128, 48} {
		var id uint32
		n, _, _ := procPrivateExtractIcon.Call(uintptr(unsafe.Pointer(name)), 0, size, size,
			uintptr(unsafe.Pointer(&hicon)), uintptr(unsafe.Pointer(&id)), 1, 0)
		if n != 0 && n != 0xFFFFFFFF && hicon != 0 {
			break
		}
		hicon = 0
	}
	if hicon == 0 {
		return nil, nil
	}
	defer procDestroyIcon.Call(hicon)

	var info iconInfo
	if r, _, _ := procGetIconInfo.Call(hicon, uintptr(unsafe.Pointer(&info))); r == 0 {
		return nil, errors.New("提取图标的物理信息失败")
	}
	if info.HbmMask != 0 {
		defer procDeleteObject.Call(info.HbmMask)
	}
	if info.HbmColor == 0 {
		return nil, errors.New("图标不含彩色位图层")
	}
	defer procDeleteObject.Call(info.HbmColor)

	hdc, _, _ := procCreateCompatibleDC.Call(0)
	if hdc == 0 {
		return nil, errors.New("创建兼容设备上下文失败")
	}
	defer procDeleteDC.Call(hdc)

	// 1. 探测位图尺寸
	var bmi bitmapInfo
	bmi.Header.Size = uint32(unsafe.Sizeof(bmi.Header))
	if r, _, _ := procGetDIBits.Call(hdc, info.HbmColor, 0, 0, 0, uintptr(unsafe.Pointer(&bmi)), 0); r == 0 {
		return nil, errors.New("探测位图元数据头部失败")
	}
	width := int(bmi.Header.Width)
	height := int(bmi.Header.Height)
	if height < 0 {
		height = -height
	}
	if width <= 0 || height <= 0 {
		return nil, errors.New("图标尺寸无效")
	}

	// 2. 以 32 位自顶向下 DIB 读取像素（BGRA 顺序）
	bmi.Header.BitCount = 32
	bmi.Header.Compression = 0 // BI_RGB
	bmi.Header.SizeImage = uint32(width * height * 4)
	bmi.Header.Height = -int32(height)
	buf := make([]byte, width*height*4)
	if r, _, _ := procGetDIBits.Call(hdc, info.HbmColor, 0, uintptr(height),
		uintptr(unsafe.Pointer(&buf[0])), uintptr(unsafe.Pointer(&bmi)), 0); r == 0 {
		return nil, errors.New("拷贝图形物理像素流失败")
	}

	// 3. BGRA → RGBA；所有像素 alpha 都为 0 说明图标没有透明通道，按不透明处理
	hasAlpha := false
	for i := 3; i < len(buf); i += 4 {
		if buf[i] > 0 {
			hasAlpha = true
			break
		}
	}
	img := image.NewNRGBA(image.Rect(0, 0, width, height))
	for i := 0; i < width*height; i++ {
		b, g, r, a := buf[i*4], buf[i*4+1], buf[i*4+2], buf[i*4+3]
		if !hasAlpha {
			a = 255
		}
		copy(img.Pix[i*4:], []byte{r, g, b, a})
	}

	var out bytes.Buffer
	if err := png.Encode(&out, img); err != nil {
		return nil, err
	}
	uri := "data:image/png;base64," + base64.StdEncoding.EncodeToString(out.Bytes())
	return &uri, nil
}
