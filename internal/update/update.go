// Package update 对照 GitHub Releases（无 Release 时回退到 tags）检查新版本。
package update

import (
	"context"
	"encoding/json"
	"fmt"
	"io"
	"log/slog"
	"net/http"
	"strconv"
	"strings"
	"time"
)

const (
	githubRepo         = "ZeroDevi1/gamesave-manager"
	githubReleasesAPI  = "https://api.github.com/repos/" + githubRepo + "/releases/latest"
	githubTagsAPI      = "https://api.github.com/repos/" + githubRepo + "/tags"
	githubReleasesPage = "https://github.com/" + githubRepo + "/releases"
	githubRepoPage     = "https://github.com/" + githubRepo
)

// CheckResult 是更新检查结果。
type CheckResult struct {
	CurrentVersion string  `json:"current_version"`
	LatestVersion  *string `json:"latest_version,omitempty"`
	HasUpdate      bool    `json:"has_update"`
	// ReleaseURL 是可交给 ReleasePageURL 打开的发布页地址。
	ReleaseURL   string  `json:"release_url"`
	ReleaseNotes *string `json:"release_notes,omitempty"`
	PublishedAt  *string `json:"published_at,omitempty"`
	Message      string  `json:"message"`
}

// versionTuple 把 "v1.2.3"、"1.2.3-beta" 解析为可比较的三元组，每段只取开头的数字。
func versionTuple(s string) [3]uint64 {
	cleaned := strings.TrimLeft(strings.TrimSpace(s), "vV")
	parts := strings.FieldsFunc(cleaned, func(r rune) bool { return r == '.' || r == '-' || r == '+' })
	var out [3]uint64
	for i := 0; i < 3 && i < len(parts); i++ {
		end := 0
		for end < len(parts[i]) && parts[i][end] >= '0' && parts[i][end] <= '9' {
			end++
		}
		out[i], _ = strconv.ParseUint(parts[i][:end], 10, 64)
	}
	return out
}

func isNewer(latest, current string) bool {
	a, b := versionTuple(latest), versionTuple(current)
	for i := range a {
		if a[i] != b[i] {
			return a[i] > b[i]
		}
	}
	return false
}

func getJSON(ctx context.Context, client *http.Client, url, version string, out any) (found bool, err error) {
	req, err := http.NewRequestWithContext(ctx, http.MethodGet, url, nil)
	if err != nil {
		return false, err
	}
	req.Header.Set("Accept", "application/vnd.github+json")
	req.Header.Set("User-Agent", "GameSave-Manager/"+version)
	resp, err := client.Do(req)
	if err != nil {
		return false, err
	}
	defer resp.Body.Close()
	if resp.StatusCode == http.StatusNotFound {
		return false, nil
	}
	if resp.StatusCode < 200 || resp.StatusCode >= 300 {
		body, _ := io.ReadAll(io.LimitReader(resp.Body, 800))
		return false, fmt.Errorf("GitHub 请求失败 (HTTP %d): %s", resp.StatusCode, body)
	}
	return true, json.NewDecoder(resp.Body).Decode(out)
}

// Check 以 current 为当前版本检查更新：先读最新 Release（草稿与预发布不算更新），
// 读取失败或没有 Release 时回退到最新 tag；两者都没有时返回提示信息而非错误。
func Check(ctx context.Context, current string) (CheckResult, error) {
	client := &http.Client{Timeout: 15 * time.Second}

	var rel struct {
		TagName     string  `json:"tag_name"`
		HTMLURL     *string `json:"html_url"`
		Body        *string `json:"body"`
		PublishedAt *string `json:"published_at"`
		Draft       bool    `json:"draft"`
		Prerelease  bool    `json:"prerelease"`
	}
	found, err := getJSON(ctx, client, githubReleasesAPI, current, &rel)
	switch {
	case err != nil:
		slog.Warn("[update] 读取 GitHub Release 失败，回退到 tags", "err", err)
	case found && !rel.Draft:
		latest := strings.TrimSpace(rel.TagName)
		has := isNewer(latest, current) && !rel.Prerelease
		url := githubReleasesPage
		if rel.HTMLURL != nil {
			url = *rel.HTMLURL
		}
		var notes *string
		if rel.Body != nil && strings.TrimSpace(*rel.Body) != "" {
			notes = rel.Body
		}
		return CheckResult{
			CurrentVersion: current, LatestVersion: &latest, HasUpdate: has, ReleaseURL: url,
			ReleaseNotes: notes, PublishedAt: rel.PublishedAt, Message: message(has, latest, current),
		}, nil
	}

	var tags []struct {
		Name string `json:"name"`
	}
	found, err = getJSON(ctx, client, githubTagsAPI, current, &tags)
	if err != nil {
		return CheckResult{}, fmt.Errorf("检查更新失败: %v", err)
	}
	if !found || len(tags) == 0 {
		return CheckResult{
			CurrentVersion: current, ReleaseURL: githubRepoPage,
			Message: fmt.Sprintf("当前版本 %s。仓库 %s 尚未发布 Release，可稍后在 GitHub 查看。", current, githubRepo),
		}, nil
	}
	tag := tags[0].Name
	has := isNewer(tag, current)
	return CheckResult{CurrentVersion: current, LatestVersion: &tag, HasUpdate: has, ReleaseURL: githubReleasesPage, Message: message(has, tag, current)}, nil
}

func message(has bool, latest, current string) string {
	if has {
		return fmt.Sprintf("发现新版本 %s（当前 %s）", latest, current)
	}
	return "已是最新版本 " + current
}

// ReleasePageURL 只允许打开本仓库的页面；空串或其它地址一律改为 Releases 列表页。
func ReleasePageURL(url string) string {
	u := strings.TrimSpace(url)
	if u == "" || !strings.HasPrefix(u, githubRepoPage) {
		return githubReleasesPage
	}
	return u
}
