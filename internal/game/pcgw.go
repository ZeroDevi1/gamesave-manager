package game

import (
	"context"
	"encoding/json"
	"fmt"
	"log/slog"
	"net/http"
	"net/url"
	"regexp"
	"strconv"
	"strings"
	"sync"
	"time"
	"unicode"

	"github.com/ZeroDevi1/gamesave-manager/internal/netutil"
)

const (
	pcgwAPI        = "https://www.pcgamingwiki.com/w/api.php"
	steamStoreAPI  = "https://store.steampowered.com/api/storesearch"
	pcgwUserAgent  = "GameSaveManager/0.1.0 (https://github.com/gamesave-manager)"
	steamTranslate = 5 // 中译英最多并发翻译前 5 条结果，避免触发 Steam 限流
)

var pcgwClient = &http.Client{Timeout: 15 * time.Second}

// PcgwSearchResult 是 PCGamingWiki 搜索结果。
type PcgwSearchResult struct {
	PageName   string  `json:"page_name"`
	SteamAppID *uint64 `json:"steam_appid,omitempty"`
}

// PcgwGameDetail 是从 PCGamingWiki 页面解析出的存档信息。
type PcgwGameDetail struct {
	PageName   string  `json:"page_name"`
	SteamAppID *uint64 `json:"steam_appid,omitempty"`
	// WindowsSavePaths 是已把 {{p|appdata}} 等宏转换为占位符的 Windows 存档路径。
	WindowsSavePaths []string `json:"windows_save_paths"`
	Notes            *string  `json:"notes,omitempty"`
}

// SteamStoreItem 是 Steam 商店搜索结果。
type SteamStoreItem struct {
	Name string `json:"name"`
	ID   uint32 `json:"id"`
}

// SteamSearchResult 是多策略 Steam 搜索的结果。
type SteamSearchResult struct {
	Items []SteamStoreItem `json:"items"`
	// Query 是实际命中结果的查询词；没有结果时为空。
	Query string `json:"query"`
}

func pcgwGet(ctx context.Context, url string) (*http.Response, error) {
	req, err := http.NewRequestWithContext(ctx, http.MethodGet, url, nil)
	if err != nil {
		return nil, err
	}
	req.Header.Set("User-Agent", pcgwUserAgent)
	return pcgwClient.Do(req)
}

func hasChinese(s string) bool {
	for _, r := range s {
		if r >= 0x4e00 && r <= 0x9fa5 {
			return true
		}
	}
	return false
}

// SearchSteamStore 搜索 Steam 商店。含中文时用国区简体中文检索，
// 并把前 5 条结果的名称替换为 appdetails 返回的官方英文名，以便再去 PCGamingWiki 查询。
func SearchSteamStore(ctx context.Context, query string) ([]SteamStoreItem, error) {
	region := "cc=US&l=english"
	if hasChinese(query) {
		region = "cc=CN&l=schinese"
	}
	resp, err := pcgwGet(ctx, fmt.Sprintf("%s?term=%s&%s", steamStoreAPI, queryEscape(query), region))
	if err != nil {
		return nil, fmt.Errorf("Steam Store API 请求失败: %v", err)
	}
	body := netutil.ReadBodyString(resp)
	if resp.StatusCode < 200 || resp.StatusCode >= 300 {
		return nil, fmt.Errorf("Steam Store API 返回 HTTP %d", resp.StatusCode)
	}
	// 兼容 JSONP 包装：截取最外层花括号
	if !strings.HasPrefix(body, `"`) && !strings.HasPrefix(body, "{") {
		start := max(strings.Index(body, "{"), 0)
		end := len(body)
		if i := strings.LastIndex(body, "}"); i >= 0 {
			end = i + 1
		}
		if start < end {
			body = body[start:end]
		}
	}
	var r struct {
		Items []SteamStoreItem `json:"items"`
	}
	if err := json.Unmarshal([]byte(body), &r); err != nil {
		return nil, fmt.Errorf("解析 Steam Store 响应失败: %v | 原始: %s", err, netutil.Head(body, 200))
	}
	items := r.Items
	if items == nil {
		items = []SteamStoreItem{}
	}

	// 并发查询英文官方名
	n := min(len(items), steamTranslate)
	names := make([]string, n)
	var wg sync.WaitGroup
	for i := 0; i < n; i++ {
		wg.Add(1)
		go func(i int) {
			defer wg.Done()
			names[i] = steamEnglishName(ctx, items[i].ID)
		}(i)
	}
	wg.Wait()
	for i, name := range names {
		if name != "" {
			items[i].Name = name
		}
	}
	return items, nil
}

// steamEnglishName 返回 AppID 的英文官方名；查询失败时返回空串。
func steamEnglishName(ctx context.Context, appid uint32) string {
	resp, err := pcgwGet(ctx, fmt.Sprintf("https://store.steampowered.com/api/appdetails?appids=%d&l=english", appid))
	if err != nil {
		return ""
	}
	body := netutil.ReadBodyString(resp)
	if resp.StatusCode < 200 || resp.StatusCode >= 300 {
		return ""
	}
	var m map[string]struct {
		Success bool `json:"success"`
		Data    *struct {
			Name string `json:"name"`
		} `json:"data"`
	}
	if json.Unmarshal([]byte(body), &m) != nil {
		return ""
	}
	if info, ok := m[strconv.FormatUint(uint64(appid), 10)]; ok && info.Success && info.Data != nil {
		return info.Data.Name
	}
	return ""
}

// queryEscape 对查询参数值编码，空格编码为 %20。
func queryEscape(s string) string { return strings.ReplaceAll(url.QueryEscape(s), "+", "%20") }

// cargoQuery 执行一次 Cargo 查询并解析 Page 与 Steam AppID 列。
func cargoQuery(ctx context.Context, url, label string) ([]PcgwSearchResult, error) {
	slog.Info("[PCGW] "+label, "url", url)
	resp, err := pcgwGet(ctx, url)
	if err != nil {
		return nil, fmt.Errorf("请求 PCGamingWiki Cargo API%s 失败: %v", suffix(label), err)
	}
	body := netutil.ReadBodyString(resp)
	if resp.StatusCode < 200 || resp.StatusCode >= 300 {
		return nil, fmt.Errorf("PCGamingWiki Cargo API%s 返回 HTTP %d: %s", suffix(label), resp.StatusCode, body)
	}
	var r struct {
		CargoQuery []struct {
			Title map[string]any `json:"title"`
		} `json:"cargoquery"`
	}
	if err := json.Unmarshal([]byte(body), &r); err != nil {
		return nil, fmt.Errorf("解析 PCGamingWiki Cargo%s 响应失败: %v", suffix(label), err)
	}
	out := []PcgwSearchResult{}
	for _, row := range r.CargoQuery {
		page, _ := row.Title["Page"].(string)
		if page == "" {
			continue
		}
		res := PcgwSearchResult{PageName: page}
		// Steam AppID 可能是逗号分隔的多个值，取第一个
		if s, ok := row.Title["Steam AppID"].(string); ok {
			first, _, _ := strings.Cut(s, ",")
			if id, err := strconv.ParseUint(strings.TrimSpace(first), 10, 64); err == nil {
				res.SteamAppID = &id
			}
		}
		out = append(out, res)
	}
	return out, nil
}

func suffix(label string) string {
	if strings.Contains(label, "AppID") {
		return " (AppID)"
	}
	return ""
}

// SearchPcgw 按页面名模糊搜索 PCGamingWiki（LIKE %query%），最多 20 条。
func SearchPcgw(ctx context.Context, query string) ([]PcgwSearchResult, error) {
	url := fmt.Sprintf("%s?action=cargoquery&tables=Infobox_game&fields=Infobox_game._pageName=Page,Infobox_game.Steam_AppID&where=Infobox_game._pageName%%20LIKE%%20%%22%%25%s%%25%%22&limit=20&format=json&origin=*",
		pcgwAPI, queryEscape(query))
	return cargoQuery(ctx, url, "搜索游戏")
}

// SearchPcgwBySteamAppID 按 Steam AppID 精确查询 PCGamingWiki 页面，最多 5 条。
func SearchPcgwBySteamAppID(ctx context.Context, appid uint64) ([]PcgwSearchResult, error) {
	url := fmt.Sprintf("%s?action=cargoquery&tables=Infobox_game&fields=Infobox_game._pageName=Page,Infobox_game.Steam_AppID&where=Infobox_game.Steam_AppID%%20=%%20%d&limit=5&format=json&origin=*",
		pcgwAPI, appid)
	return cargoQuery(ctx, url, "按 AppID 查询")
}

// SearchSteamStoreRobust 渐进退化地搜索 Steam（Steam 对完整中文名常常搜不到）：
// 1) 从末尾逐字缩短查询直到有结果；2) 去掉标点只保留中文、字母、数字；3) 只保留中文。
func SearchSteamStoreRobust(ctx context.Context, query string) (SteamSearchResult, error) {
	trimmed := strings.TrimSpace(query)
	empty := SteamSearchResult{Items: []SteamStoreItem{}}
	if trimmed == "" {
		return empty, nil
	}

	current := trimmed
	for len(current) >= 2 {
		items, err := SearchSteamStore(ctx, current)
		if err != nil {
			return empty, err
		}
		if len(items) > 0 {
			slog.Info("[Steam鲁棒] 命中", "query", trimmed, "used", current, "count", len(items))
			return SteamSearchResult{Items: items, Query: current}, nil
		}
		runes := []rune(current)
		if len(runes) <= 2 {
			break
		}
		current = string(runes[:len(runes)-1])
	}

	var cleanedB, chineseB strings.Builder
	for _, r := range trimmed {
		isCJK := r >= 0x4e00 && r <= 0x9fa5
		if isCJK || isAlnum(r) {
			cleanedB.WriteRune(r)
		}
		if isCJK {
			chineseB.WriteRune(r)
		}
	}
	cleaned, chinese := cleanedB.String(), chineseB.String()

	for _, cand := range []struct {
		q   string
		use bool
	}{
		{cleaned, len(cleaned) >= 2 && cleaned != trimmed},
		{chinese, len(chinese) >= 2 && chinese != trimmed && chinese != cleaned},
	} {
		if !cand.use {
			continue
		}
		items, err := SearchSteamStore(ctx, cand.q)
		if err != nil {
			return empty, err
		}
		if len(items) > 0 {
			return SteamSearchResult{Items: items, Query: cand.q}, nil
		}
	}
	return empty, nil
}

// FetchSavePaths 读取 PCGamingWiki 页面 wikitext，解析 Windows 存档路径、备注与 Steam AppID。
// 页面不存在时用 Cargo 搜索取最接近的页面名再试一次。
func FetchSavePaths(ctx context.Context, pageName string) (PcgwGameDetail, error) {
	return fetchSavePaths(ctx, pageName, true)
}

func fetchSavePaths(ctx context.Context, pageName string, allowFallback bool) (PcgwGameDetail, error) {
	// Steam 商店名常带商标符号，PCGW 页面名没有
	queryName := strings.NewReplacer("®", "", "™", "").Replace(pageName)
	if queryName == "" {
		queryName = pageName
	}
	url := fmt.Sprintf("%s?action=parse&page=%s&prop=wikitext&format=json&origin=*", pcgwAPI, queryEscape(queryName))
	slog.Info("[PCGW] 获取存档路径", "page", queryName)
	resp, err := pcgwGet(ctx, url)
	if err != nil {
		return PcgwGameDetail{}, fmt.Errorf("请求 PCGamingWiki Parse API 失败: %v", err)
	}
	body := netutil.ReadBodyString(resp)
	if resp.StatusCode < 200 || resp.StatusCode >= 300 {
		return PcgwGameDetail{}, fmt.Errorf("PCGamingWiki Parse API 返回 HTTP %d: %s", resp.StatusCode, body)
	}
	var r struct {
		Error *struct {
			Code string `json:"code"`
			Info string `json:"info"`
		} `json:"error"`
		Parse *struct {
			Wikitext map[string]string `json:"wikitext"`
		} `json:"parse"`
	}
	if err := json.Unmarshal([]byte(body), &r); err != nil {
		return PcgwGameDetail{}, fmt.Errorf("解析 PCGamingWiki Parse JSON 失败: %v | 原始文本前 500 字: %s", err, netutil.Head(body, 500))
	}
	if r.Error != nil {
		code := r.Error.Code
		if code == "" {
			code = "unknown"
		}
		if allowFallback && code == "missingtitle" {
			slog.Info("[PCGW] 页面不存在，尝试 Cargo 回退搜索", "page", queryName)
			return fetchViaCargoFallback(ctx, queryName)
		}
		return PcgwGameDetail{}, fmt.Errorf("PCGamingWiki API 错误 [%s]: %s", code, r.Error.Info)
	}
	wikitext := ""
	if r.Parse != nil {
		wikitext = r.Parse.Wikitext["*"]
	}
	if wikitext == "" {
		return PcgwGameDetail{}, fmt.Errorf("PCGamingWiki 返回了空 wikitext，页面 '%s' 可能不存在", queryName)
	}
	paths, notes := parseWindowsSavePaths(wikitext)
	return PcgwGameDetail{PageName: queryName, SteamAppID: steamAppIDFromWikitext(wikitext), WindowsSavePaths: paths, Notes: notes}, nil
}

func fetchViaCargoFallback(ctx context.Context, pageName string) (PcgwGameDetail, error) {
	candidates, err := SearchPcgw(ctx, pageName)
	if err != nil {
		return PcgwGameDetail{}, fmt.Errorf("PCGamingWiki 页面 '%s' 不存在，且 Cargo 回退搜索也失败: %v", pageName, err)
	}
	if len(candidates) == 0 {
		return PcgwGameDetail{}, fmt.Errorf("PCGamingWiki 页面 '%s' 不存在，Cargo 回退搜索也未找到匹配游戏", pageName)
	}
	fallback := candidates[0].PageName
	slog.Info("[PCGW] Cargo 回退搜索命中", "from", pageName, "to", fallback)
	d, err := fetchSavePaths(ctx, fallback, false)
	if err != nil {
		return PcgwGameDetail{}, fmt.Errorf("PCGamingWiki 页面 '%s' 不存在，回退到 '%s' 后仍然失败: %v", pageName, fallback, err)
	}
	return d, nil
}

var (
	reSaveSectionTitle = regexp.MustCompile(`(?i)==+\s*Save\s+game\s+data\s+location\s*==+`)
	reNextTitle        = regexp.MustCompile(`\n==+`)
	reSavesPrefix      = regexp.MustCompile(`(?i)^\s*\{\{\s*Game\s+data/saves\s*\|\s*Windows\s*\|\s*`)
	reSteamAppID       = regexp.MustCompile(`\|Steam AppID\s*=\s*(\d+)`)
)

// parseWindowsSavePaths 从 "Save game data location" 小节中提取 Windows 存档路径与 {{--}} 备注。
func parseWindowsSavePaths(wikitext string) ([]string, *string) {
	section := ""
	if parts := reSaveSectionTitle.Split(wikitext, -1); len(parts) > 1 {
		section = parts[1]
	}
	if loc := reNextTitle.FindStringIndex(section); loc != nil {
		section = section[:loc[0]]
	}

	paths := []string{}
	var notes []string
	for _, line := range strings.Split(section, "\n") {
		t := strings.TrimSpace(line)
		collapsed := strings.ToLower(strings.ReplaceAll(t, " ", ""))
		switch {
		case strings.HasPrefix(collapsed, "{{gamedata/saves|windows|"):
			if p := extractSavesLinePath(t); p != "" && !contains(paths, p) {
				paths = append(paths, p)
			}
		case strings.HasPrefix(t, "{{--}}"):
			note := strings.TrimRight(strings.TrimSpace(strings.TrimPrefix(t, "{{--}}")), ".")
			if note != "" {
				notes = append(notes, note)
			}
		}
	}
	if len(notes) == 0 {
		return paths, nil
	}
	joined := strings.Join(notes, "; ")
	return paths, &joined
}

func contains(s []string, v string) bool {
	for _, x := range s {
		if x == v {
			return true
		}
	}
	return false
}

// extractSavesLinePath 提取 {{Game data/saves|Windows|<路径>|备注}} 中的路径。
// 路径里可能嵌套 {{p|userprofile}} 之类的宏，因此按花括号深度扫描：
// 只有深度为 0 时遇到 "|" 或 "}" 才结束，避免在嵌套宏的右括号处截断。
func extractSavesLinePath(line string) string {
	loc := reSavesPrefix.FindStringIndex(line)
	if loc == nil {
		return ""
	}
	depth := 0
	var raw strings.Builder
	for _, c := range line[loc[1]:] {
		if depth == 0 && (c == '|' || c == '}') {
			break
		}
		switch c {
		case '{':
			depth++
		case '}':
			depth--
		}
		raw.WriteRune(c)
	}
	p := strings.TrimSpace(raw.String())
	if p == "" {
		return ""
	}
	return convertPcgwPlaceholders(p)
}

var pcgwPlaceholders = []struct {
	re *regexp.Regexp
	to string
}{
	{regexp.MustCompile(`(?i)\{\{\s*p\s*\|\s*appdata\s*\}\}`), "%APPDATA%"},
	{regexp.MustCompile(`(?i)\{\{\s*p\s*\|\s*localappdata\s*\}\}`), "%LOCALAPPDATA%"},
	{regexp.MustCompile(`(?i)\{\{\s*p\s*\|\s*userprofile\\documents\s*\}\}`), "%USERPROFILE%/Documents"},
	{regexp.MustCompile(`(?i)\{\{\s*p\s*\|\s*userprofile/documents\s*\}\}`), "%USERPROFILE%/Documents"},
	{regexp.MustCompile(`(?i)\{\{\s*p\s*\|\s*userprofile\s*\}\}`), "%USERPROFILE%"},
	{regexp.MustCompile(`(?i)\{\{\s*p\s*\|\s*programfiles\s*\}\}`), "%PROGRAMFILES%"},
	{regexp.MustCompile(`(?i)\{\{\s*p\s*\|\s*public\s*\}\}`), "%PUBLIC%"},
	{regexp.MustCompile(`(?i)\{\{\s*p\s*\|\s*steam\s*\}\}`), "%STEAMPATH%"},
	// Ubisoft Connect 默认安装在 Program Files (x86) 下
	{regexp.MustCompile(`(?i)\{\{\s*p\s*\|\s*uplay\s*\}\}`), "%PROGRAMFILES(X86)%/Ubisoft Game Launcher"},
	// UID 是动态数字，替换为通配符交给 glob 匹配
	{regexp.MustCompile(`(?i)\{\{\s*p\s*\|\s*uid\s*\}\}`), "*"},
	{regexp.MustCompile(`(?i)\{\{\s*p\s*\|\s*game\s*\}\}`), "<path-to-game>"},
}

// convertPcgwPlaceholders 把 PCGW 路径宏转换为环境变量占位符，并统一为正斜杠。
func convertPcgwPlaceholders(text string) string {
	for _, p := range pcgwPlaceholders {
		text = p.re.ReplaceAllLiteralString(text, p.to)
	}
	return strings.ReplaceAll(text, `\`, "/")
}

func steamAppIDFromWikitext(wikitext string) *uint64 {
	m := reSteamAppID.FindStringSubmatch(wikitext)
	if m == nil {
		return nil
	}
	id, err := strconv.ParseUint(m[1], 10, 64)
	if err != nil {
		return nil
	}
	return &id
}

// isAlnum 对应 Rust char::is_alphanumeric（Unicode 字母或数字）。
func isAlnum(r rune) bool { return unicode.IsLetter(r) || unicode.IsNumber(r) }
