package main

import (
	"context"
	"encoding/json"
	"errors"
	"fmt"
	"time"

	"github.com/egoist/mygo"

	"github.com/ZeroDevi1/gamesave-manager/internal/storage"
)

const (
	// oplistPollInterval 是向授权页注入脚本、检查令牌是否生成的间隔。
	oplistPollInterval = time.Second
	// oplistTimeout 是授权流程的总时限，超时后关闭小窗并报错。
	oplistTimeout = 10 * time.Minute
	// oplistEvalTimeout 限制单次注入等待；页面跳转期间脚本可能拿不到结果。
	oplistEvalTimeout = 3 * time.Second
)

// oplistScript 在 api.oplist.org 页面中执行（%s 为 JSON 编码的驱动名）：
// 1) 在下拉框中选中与驱动匹配的网盘；2) 勾选“使用 OpenList 提供的参数”；
// 3) access-token 尚无值时点击“获取 Token”（有值后不再点击，否则会清空刚生成的令牌）；
// 4) 令牌出现后返回 {access_token, refresh_token}，否则返回 null。
const oplistScript = `
const driver = %s;
const selectEl = document.querySelector('select');
if (selectEl && !selectEl.dataset.autoSelected) {
  for (let i = 0; i < selectEl.options.length; i++) {
    const t = selectEl.options[i].text.toLowerCase();
    const v = selectEl.options[i].value.toLowerCase();
    const has = (...keys) => keys.some((k) => t.includes(k) || v.includes(k));
    const match =
      (driver.includes('alicloud') && has('阿里云', 'alicloud', 'aliyun')) ||
      (driver.includes('baidu') && has('百度', 'baidu')) ||
      (driver.includes('onedrive') && has('onedrive')) ||
      (driver.includes('quark') && has('夸克', 'quark'));
    if (match) {
      selectEl.selectedIndex = i;
      selectEl.dispatchEvent(new Event('change', { bubbles: true }));
      selectEl.dataset.autoSelected = 'true';
      break;
    }
  }
}
const checkbox = document.querySelector('input[type="checkbox"]');
if (checkbox && !checkbox.checked && !checkbox.dataset.autoChecked) {
  checkbox.checked = true;
  checkbox.dispatchEvent(new Event('change', { bubbles: true }));
  checkbox.dispatchEvent(new Event('click', { bubbles: true }));
  checkbox.dataset.autoChecked = 'true';
}
const tokenEl = document.getElementById('access-token');
const hasToken = tokenEl && tokenEl.value && tokenEl.value.trim().length > 15;
if (!hasToken) {
  for (const btn of document.querySelectorAll('button')) {
    const text = btn.textContent || btn.innerText;
    if (text && (text.includes('获取 Token') || text.includes('获取Token')) && !btn.dataset.autoClicked) {
      btn.click();
      btn.dataset.autoClicked = 'true';
      break;
    }
  }
  return null;
}
const refreshEl = document.getElementById('refresh-token');
return { access_token: tokenEl.value.trim(), refresh_token: refreshEl ? refreshEl.value.trim() : '' };
`

// runOplistAuth 打开授权小窗并轮询注入脚本，直到捕获令牌、用户关闭窗口（返回 nil, nil）、
// 调用方取消或超时（返回错误并关闭窗口）。
func runOplistAuth(ctx context.Context, driver string) (*storage.NetdiskTokens, error) {
	driverJSON, err := json.Marshal(driver)
	if err != nil {
		return nil, err
	}
	script := fmt.Sprintf(oplistScript, driverJSON)

	win := mygo.NewWindow(mygo.WindowOptions{
		Title:         "🔐 网盘一键扫码云授权",
		URL:           "https://api.oplist.org/",
		Width:         480,
		Height:        640,
		DisableResize: true,
		AlwaysOnTop:   true,
	})
	closed := make(chan struct{})
	win.OnClosed(func() { close(closed) })

	deadline := time.After(oplistTimeout)
	ticker := time.NewTicker(oplistPollInterval)
	defer ticker.Stop()
	for {
		select {
		case <-closed:
			return nil, nil
		case <-ctx.Done():
			win.Destroy()
			return nil, ctx.Err()
		case <-deadline:
			win.Destroy()
			return nil, errors.New("网盘授权超时（10 分钟），请重试")
		case <-ticker.C:
			if tokens := evalTokens(ctx, win, script); tokens != nil {
				win.Destroy()
				return tokens, nil
			}
		}
	}
}

// evalTokens 执行一次注入脚本；页面跳转中、脚本出错或尚无令牌时返回 nil。
func evalTokens(ctx context.Context, win *mygo.Window, script string) *storage.NetdiskTokens {
	page := win.Page()
	if page == nil {
		return nil
	}
	evalCtx, cancel := context.WithTimeout(ctx, oplistEvalTimeout)
	defer cancel()
	v, err := page.EvalContext(evalCtx, script)
	if err != nil || v == nil {
		return nil
	}
	raw, err := json.Marshal(v)
	if err != nil {
		return nil
	}
	var t storage.NetdiskTokens
	if json.Unmarshal(raw, &t) != nil || t.AccessToken == "" {
		return nil
	}
	return &t
}
