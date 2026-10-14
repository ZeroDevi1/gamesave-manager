package storage

import (
	"encoding/json"
	"log/slog"
	"regexp"
	"strings"
)

var (
	reNetscapeLine = regexp.MustCompile(`^\S+\s+(?:TRUE|FALSE)\s+\S+\s+(?:TRUE|FALSE)\s+\d+\s+\S+\s+\S+`)
	reWhitespace   = regexp.MustCompile(`\s+`)
)

// quarkKnownDomains 用于在“换行符丢失”的 Netscape 文本中定位行边界。
var quarkKnownDomains = []string{"pan.quark.cn", ".quark.cn"}

// lines 按 "\n" 切分并去掉行尾 "\r"（与 Rust str::lines 一致）。
func lines(s string) []string {
	parts := strings.Split(s, "\n")
	for i, p := range parts {
		parts[i] = strings.TrimSuffix(p, "\r")
	}
	if n := len(parts); n > 0 && parts[n-1] == "" {
		parts = parts[:n-1]
	}
	return parts
}

// parseQuarkCookie 把用户粘贴的夸克 Cookie 转换为 HTTP Cookie 头格式 "name=value; name=value"。
//
// 支持三种输入：浏览器扩展导出的 JSON 数组、Netscape cookie 文件（含换行符被压掉的单行变体），
// 以及已是 HTTP 头格式的字符串（原样返回）。isQuark / isQuark.sig 两个标记 Cookie 会被丢弃。
func parseQuarkCookie(raw string) string { return parseQuarkCookieDepth(raw, true) }

func parseQuarkCookieDepth(raw string, allowRebuild bool) string {
	trimmed := strings.TrimSpace(raw)

	// 1. JSON 数组（EditThisCookie 等扩展导出）
	if strings.HasPrefix(trimmed, "[") || strings.HasPrefix(trimmed, "{") {
		var arr []map[string]any
		if json.Unmarshal([]byte(trimmed), &arr) == nil {
			var pairs []string
			for _, item := range arr {
				name, ok1 := item["name"].(string)
				value, ok2 := item["value"].(string)
				if !ok1 || !ok2 || name == "isQuark" || name == "isQuark.sig" {
					continue
				}
				pairs = append(pairs, name+"="+value)
			}
			if len(pairs) > 0 {
				return strings.Join(pairs, "; ")
			}
		}
	}

	// 2. Netscape 多行格式：domain flag path secure expires name value
	hasTabs := strings.Contains(trimmed, "\t")
	all := lines(trimmed)
	if !hasTabs || len(all) > 1 {
		var pairs []string
		matched := 0
		for _, l := range all {
			lt := strings.TrimSpace(l)
			if lt == "" || strings.HasPrefix(lt, "#") || !reNetscapeLine.MatchString(lt) {
				continue
			}
			matched++
			parts := reWhitespace.Split(lt, -1)
			if len(parts) < 7 {
				continue
			}
			name, value := parts[5], parts[6]
			if name == "isQuark" || name == "isQuark.sig" || name == "" {
				continue
			}
			pairs = append(pairs, name+"="+value)
		}
		if len(pairs) > 0 {
			slog.Info("[Quark] 从 Netscape 格式解析出 Cookie", "count", len(pairs), "lines", matched)
			return strings.Join(pairs, "; ")
		}
	}

	// 3. 换行符丢失的变体：整段被压成一行、仍以制表符分隔，每行末尾的 value 与下一行开头的 domain 粘连。
	// 在域名边界重新插入换行后再按多行格式解析；只重建一次，避免重建结果不变时无限递归。
	if hasTabs && allowRebuild {
		rebuilt := rebuildNetscape(trimmed)
		reparsed := parseQuarkCookieDepth(rebuilt, false)
		if reparsed != rebuilt && reparsed != trimmed {
			slog.Info("[Quark] 换行符丢失变体: 重建格式后成功解析")
			return reparsed
		}
		slog.Warn("[Quark] 换行符丢失变体: 重建格式后解析仍失败，按原文使用")
	}

	// 4. 视为已是 HTTP Cookie 头格式
	return trimmed
}

// rebuildNetscape 在已知域名处切开粘连字段（"valuepan.quark.cn" → "value\npan.quark.cn"），
// 恢复 Netscape 文件的多行结构。
func rebuildNetscape(flat string) string {
	var sb strings.Builder
	needNewline := false
	endsWithNewline := func() bool { s := sb.String(); return strings.HasSuffix(s, "\n") }

	for i, part := range strings.Split(flat, "\t") {
		t := strings.TrimSpace(part)
		if t == "" || strings.HasPrefix(t, "#") {
			// 注释或空字段：独占一行
			if sb.Len() > 0 {
				sb.WriteByte('\n')
			}
			sb.WriteString(part)
			needNewline = false
			continue
		}

		handled := false
		for _, domain := range quarkKnownDomains {
			if t == domain {
				// 独立的域名字段：新的一行从这里开始
				if i > 0 && needNewline {
					sb.WriteByte('\n')
				}
				if sb.Len() > 0 && !endsWithNewline() {
					sb.WriteByte('\t')
				}
				sb.WriteString(t)
				needNewline, handled = true, true
				break
			}
			if pos := strings.LastIndex(t, domain); pos > 0 {
				// 粘连字段：前半是上一行的 value，后半是下一行的 domain
				if sb.Len() > 0 {
					sb.WriteByte('\t')
				}
				sb.WriteString(t[:pos])
				sb.WriteByte('\n')
				sb.WriteString(t[pos:])
				needNewline, handled = true, true
				break
			}
		}
		if !handled {
			if sb.Len() > 0 && !endsWithNewline() {
				sb.WriteByte('\t')
			}
			sb.WriteString(part)
		}
	}
	return sb.String()
}

// setCookieValue 在 Cookie 头字符串中替换 key 的值，不存在时追加到末尾。
func setCookieValue(cookie, key, value string) string {
	pair := key + "=" + value
	found := false
	items := strings.Split(cookie, ";")
	for i, item := range items {
		t := strings.TrimSpace(item)
		if k, _, ok := strings.Cut(t, "="); ok && strings.TrimSpace(k) == key {
			items[i] = pair
			found = true
		} else {
			items[i] = t
		}
	}
	if found {
		return strings.Join(items, "; ")
	}
	if strings.TrimSpace(cookie) == "" {
		return pair
	}
	return strings.TrimSpace(strings.TrimRight(cookie, ";")) + "; " + pair
}
