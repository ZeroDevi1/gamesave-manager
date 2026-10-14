package storage

import (
	"bytes"
	"context"
	"crypto/md5"
	"crypto/sha256"
	"encoding/hex"
	"encoding/json"
	"errors"
	"fmt"
	"net/http"
	"strconv"
	"strings"
	"time"

	"github.com/ZeroDevi1/gamesave-manager/internal/netutil"
)

// 夸克 TV 扫码登录参数，来自 OpenList 的夸克 TV 驱动。
const (
	quarkTVAPI      = "https://open-api-drive.quark.cn"
	quarkTVClientID = "d3194e61504e493eb6222857bccfed94"
	quarkTVSignKey  = "kw2dvtd7p4t3pjl2d9ed9yc8yej8kw2d"
	quarkTVAppVer   = "1.8.2.2"
	quarkTVChannel  = "GENERAL"
	quarkTVUA       = "Mozilla/5.0 (Linux; U; Android 13; zh-cn; M2004J7AC Build/UKQ1.231108.001) AppleWebKit/533.1 (KHTML, like Gecko) Mobile Safari/533.1"
	quarkTVTokenURL = "http://api.extscreen.com/quarkdrive/token"
)

// 扫码登录时还没有 access token，设备 ID 由空 Token 派生，因此对所有用户是同一个固定值。
var quarkTVDeviceID = md5Hex("")

func md5Hex(s string) string {
	sum := md5.Sum([]byte(s))
	return hex.EncodeToString(sum[:])
}

// quarkTVSign 返回 (时间戳毫秒, x-pan-token 签名, req_id)。
func quarkTVSign(method, path string) (string, string, string) {
	tm := strconv.FormatInt(time.Now().UnixMilli(), 10)
	reqID := md5Hex(quarkTVDeviceID + tm)
	sum := sha256.Sum256([]byte(method + "&" + path + "&" + tm + "&" + quarkTVSignKey))
	return tm, hex.EncodeToString(sum[:]), reqID
}

type quarkTVCommon struct {
	Status    *int    `json:"status"`
	Errno     *int    `json:"errno"`
	ErrorInfo *string `json:"error_info"`
}

var quarkTVClient = &http.Client{Timeout: 30 * time.Second}

// quarkTVGet 发送带设备指纹与签名的 GET 请求，返回响应文本。
func quarkTVGet(ctx context.Context, path string, params [][2]string) (string, error) {
	tm, token, reqID := quarkTVSign(http.MethodGet, path)
	var sb strings.Builder
	fmt.Fprintf(&sb, "%s%s?req_id=%s&access_token=&app_ver=%s&device_id=%s&device_brand=Xiaomi&platform=tv&device_name=M2004J7AC&device_model=M2004J7AC&build_device=M2004J7AC&build_product=M2004J7AC&device_gpu=Adreno%%20(TM)%%20550&activity_rect={}&channel=%s",
		quarkTVAPI, path, reqID, quarkTVAppVer, queryEscape(quarkTVDeviceID), quarkTVChannel)
	for _, p := range params {
		sb.WriteString("&" + p[0] + "=" + queryEscape(p[1]))
	}
	req, err := http.NewRequestWithContext(ctx, http.MethodGet, sb.String(), nil)
	if err != nil {
		return "", err
	}
	req.Header.Set("Accept", "application/json, text/plain, */*")
	req.Header.Set("User-Agent", quarkTVUA)
	req.Header.Set("x-pan-tm", tm)
	req.Header.Set("x-pan-token", token)
	req.Header.Set("x-pan-client-id", quarkTVClientID)
	resp, err := quarkTVClient.Do(req)
	if err != nil {
		return "", err
	}
	return netutil.ReadBodyString(resp), nil
}

// QuarkTVQRCode 是夸克 TV 登录二维码。
type QuarkTVQRCode struct {
	// QRData 是 PNG 图片的 base64（不含 data: 前缀）。
	QRData string `json:"qr_data"`
	// QueryToken 用于轮询授权状态。
	QueryToken string `json:"query_token"`
}

// QuarkTVGetQRCode 获取扫码登录二维码。
func QuarkTVGetQRCode(ctx context.Context) (QuarkTVQRCode, error) {
	body, err := quarkTVGet(ctx, "/oauth/authorize", [][2]string{
		{"auth_type", "code"}, {"client_id", quarkTVClientID}, {"scope", "netdisk"},
		{"qrcode", "1"}, {"qr_width", "460"}, {"qr_height", "460"},
	})
	if err != nil {
		return QuarkTVQRCode{}, err
	}
	var r struct {
		quarkTVCommon
		QRData     *string `json:"qr_data"`
		QueryToken *string `json:"query_token"`
	}
	if err := json.Unmarshal([]byte(body), &r); err != nil {
		return QuarkTVQRCode{}, fmt.Errorf("解析夸克 TV 二维码响应失败: %v", err)
	}
	status, errno := deref(r.Status), deref(r.Errno)
	if status >= 400 || errno != 0 {
		msg := body
		if r.ErrorInfo != nil {
			msg = *r.ErrorInfo
		}
		return QuarkTVQRCode{}, fmt.Errorf("夸克 TV 获取二维码失败 (status=%d, errno=%d): %s", status, errno, msg)
	}
	if r.QRData == nil {
		return QuarkTVQRCode{}, errors.New("二维码数据为空")
	}
	if r.QueryToken == nil {
		return QuarkTVQRCode{}, errors.New("query_token 为空")
	}
	return QuarkTVQRCode{QRData: *r.QRData, QueryToken: *r.QueryToken}, nil
}

// QuarkTVPollCode 轮询扫码授权；用户尚未确认时返回 nil。
func QuarkTVPollCode(ctx context.Context, queryToken string) (*string, error) {
	body, err := quarkTVGet(ctx, "/oauth/code", [][2]string{
		{"client_id", quarkTVClientID}, {"scope", "netdisk"}, {"query_token", queryToken},
	})
	if err != nil {
		return nil, err
	}
	var r struct {
		quarkTVCommon
		Code *string `json:"code"`
	}
	if err := json.Unmarshal([]byte(body), &r); err != nil {
		return nil, fmt.Errorf("解析夸克 TV 授权码响应失败: %v", err)
	}
	if deref(r.Status) >= 400 {
		msg := ""
		if r.ErrorInfo != nil {
			msg = *r.ErrorInfo
		}
		if strings.Contains(msg, "未授权") || strings.Contains(msg, "not authorized") || strings.Contains(msg, "waiting") {
			return nil, nil
		}
		return nil, fmt.Errorf("夸克 TV 获取授权码失败: %s", msg)
	}
	return r.Code, nil
}

// NetdiskTokens 是一对访问令牌与刷新令牌。
type NetdiskTokens struct {
	AccessToken  string `json:"access_token"`
	RefreshToken string `json:"refresh_token"`
}

// QuarkTVExchange 用授权码换取访问令牌与刷新令牌。
func QuarkTVExchange(ctx context.Context, code string) (NetdiskTokens, error) {
	_, _, reqID := quarkTVSign(http.MethodPost, "/token")
	payload, _ := json.Marshal(map[string]string{
		"req_id": reqID, "app_ver": quarkTVAppVer, "device_id": quarkTVDeviceID,
		"device_brand": "Xiaomi", "platform": "tv", "device_name": "M2004J7AC",
		"device_model": "M2004J7AC", "build_device": "M2004J7AC",
		"build_product": "M2004J7AC", "device_gpu": "Adreno (TM) 550",
		"activity_rect": "{}", "channel": quarkTVChannel, "code": code,
	})
	req, err := http.NewRequestWithContext(ctx, http.MethodPost, quarkTVTokenURL, bytes.NewReader(payload))
	if err != nil {
		return NetdiskTokens{}, err
	}
	req.Header.Set("Content-Type", "application/json")
	resp, err := quarkTVClient.Do(req)
	if err != nil {
		return NetdiskTokens{}, err
	}
	body := netutil.ReadBodyString(resp)
	var r struct {
		Code    *int    `json:"code"`
		Message *string `json:"message"`
		Data    *struct {
			AccessToken  *string `json:"access_token"`
			RefreshToken *string `json:"refresh_token"`
		} `json:"data"`
	}
	if err := json.Unmarshal([]byte(body), &r); err != nil {
		return NetdiskTokens{}, fmt.Errorf("解析夸克 TV Token 响应失败: %v", err)
	}
	if deref(r.Code) != 200 {
		msg := body
		if r.Message != nil {
			msg = *r.Message
		}
		return NetdiskTokens{}, fmt.Errorf("夸克 TV Token 交换失败: %s", msg)
	}
	if r.Data == nil {
		return NetdiskTokens{}, errors.New("Token 响应中无 data 字段")
	}
	if r.Data.AccessToken == nil {
		return NetdiskTokens{}, errors.New("未获取到 access_token")
	}
	t := NetdiskTokens{AccessToken: *r.Data.AccessToken}
	if r.Data.RefreshToken != nil {
		t.RefreshToken = *r.Data.RefreshToken
	}
	return t, nil
}
