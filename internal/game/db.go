// Package game 管理游戏：游戏数据库（存档路径模板）、PCGamingWiki/Steam 查询、封面、exe 图标提取、
// 本地存档扫描、Steam 启动与游戏进程监控。
package game

import (
	"encoding/json"
	"errors"
	"io/fs"
	"os"
	"path/filepath"
	"strings"
	"sync"

	"github.com/ZeroDevi1/gamesave-manager/internal/appdir"
	"github.com/ZeroDevi1/gamesave-manager/internal/pathutil"
)

// GameDbEntry 是游戏数据库中的一个存档路径模板。
type GameDbEntry struct {
	ID   string `json:"id"`
	Name string `json:"name"`
	// Aliases 是别名与可执行文件名；第一个通常是主程序，用作进程监控的 exe 名。
	Aliases []string `json:"aliases"`
	// SavePaths 是含 %APPDATA% 等占位符的存档路径模板。
	SavePaths  []string `json:"save_paths"`
	Platforms  []string `json:"platforms"`
	SteamAppID *uint64  `json:"steam_appid,omitempty"`
	Notes      *string  `json:"notes,omitempty"`
	// Source 取 "builtin" 或 "user"。
	Source string `json:"source"`
}

// GameDatabase 是 game_database.json 的内容。
type GameDatabase struct {
	Entries []GameDbEntry `json:"entries"`
	Version uint32        `json:"version"`
}

func u64(v uint64) *uint64 { return &v }
func str(s string) *string { return &s }

// builtinEntries 返回内置的常见游戏存档路径模板。
func builtinEntries() []GameDbEntry {
	e := func(id, name string, aliases []string, path string, appid uint64, notes *string) GameDbEntry {
		return GameDbEntry{ID: id, Name: name, Aliases: aliases, SavePaths: []string{path}, Platforms: []string{"windows"}, SteamAppID: u64(appid), Notes: notes, Source: "builtin"}
	}
	return []GameDbEntry{
		e("elden-ring", "Elden Ring", []string{"eldenring.exe"}, "%APPDATA%/EldenRing", 1245620, str(`存档位于 AppData\Roaming\EldenRing`)),
		e("witcher-3", "The Witcher 3: Wild Hunt", []string{"witcher3.exe"}, "%USERPROFILE%/Documents/The Witcher 3/gamesaves", 292030, str("Steam 版与 GOG 版路径相同")),
		e("dark-souls-3", "Dark Souls III", []string{"DarkSoulsIII.exe"}, "%APPDATA%/DarkSoulsIII", 374320, nil),
		e("baldurs-gate-3", "Baldur's Gate 3", []string{"bg3.exe", "bg3_dx11.exe"}, "%LOCALAPPDATA%/Larian Studios/Baldur's Gate 3/PlayerProfiles/Public/Savegames", 1086940, str("Profile 名称可能不同，默认是 Public")),
		e("stardew-valley", "Stardew Valley", []string{"Stardew Valley.exe"}, "%APPDATA%/StardewValley/Saves", 413150, nil),
		e("cyberpunk-2077", "Cyberpunk 2077", []string{"Cyberpunk2077.exe"}, "%LOCALAPPDATA%/CD Projekt Red/Cyberpunk 2077", 1091500, nil),
		e("hollow-knight", "Hollow Knight", []string{"hollow_knight.exe"}, "%APPDATA%/unity3d/Team Cherry/Hollow Knight", 367520, nil),
		e("hades", "Hades", []string{"Hades.exe"}, "%LOCALAPPDATA%/Hades/Saved/SaveGames", 1145360, nil),
		e("sekiro", "Sekiro: Shadows Die Twice", []string{"sekiro.exe"}, "%APPDATA%/Sekiro", 814380, nil),
		e("hades-2", "Hades II", []string{"Hades2.exe"}, "%LOCALAPPDATA%/Hades II/Saved/SaveGames", 1145350, nil),
	}
}

func defaultDB() GameDatabase { return GameDatabase{Entries: builtinEntries(), Version: 1} }

// dbMu 串行化 game_database.json 的读写。
var dbMu sync.Mutex

func dbPath() (string, error) {
	dir, err := appdir.Dir()
	if err != nil {
		return "", err
	}
	return filepath.Join(dir, "game_database.json"), nil
}

// LoadDB 读取游戏数据库，并补上文件中缺少的内置条目；文件不存在时返回内置数据库。
func LoadDB() (GameDatabase, error) {
	p, err := dbPath()
	if err != nil {
		return GameDatabase{}, err
	}
	dbMu.Lock()
	data, err := os.ReadFile(p)
	dbMu.Unlock()
	if errors.Is(err, fs.ErrNotExist) {
		return defaultDB(), nil
	}
	if err != nil {
		return GameDatabase{}, err
	}
	db := defaultDB()
	db.Entries = nil
	if err := json.Unmarshal(data, &db); err != nil {
		return GameDatabase{}, err
	}
	return mergeBuiltin(db), nil
}

// SaveDB 覆盖写入游戏数据库。
func SaveDB(db *GameDatabase) error {
	p, err := dbPath()
	if err != nil {
		return err
	}
	data, err := json.MarshalIndent(db, "", "  ")
	if err != nil {
		return err
	}
	dbMu.Lock()
	defer dbMu.Unlock()
	return os.WriteFile(p, data, 0o644)
}

func mergeBuiltin(db GameDatabase) GameDatabase {
	existing := map[string]bool{}
	for _, e := range db.Entries {
		existing[e.ID] = true
	}
	for _, b := range builtinEntries() {
		if !existing[b.ID] {
			db.Entries = append(db.Entries, b)
		}
	}
	if db.Entries == nil {
		db.Entries = []GameDbEntry{}
	}
	return db
}

// SearchEntries 按名称、别名或 ID 做不区分大小写的子串匹配。
func SearchEntries(db *GameDatabase, query string) []GameDbEntry {
	q := strings.ToLower(query)
	out := []GameDbEntry{}
	for _, e := range db.Entries {
		match := strings.Contains(strings.ToLower(e.Name), q) || strings.Contains(strings.ToLower(e.ID), q)
		for _, a := range e.Aliases {
			match = match || strings.Contains(strings.ToLower(a), q)
		}
		if match {
			out = append(out, e)
		}
	}
	return out
}

// UpsertEntry 新增或按 ID 替换条目。保存前把存档路径中属于本机用户的目录折叠为占位符，
// 使导出的模板能在其它机器上使用。返回实际保存的条目。
func UpsertEntry(entry GameDbEntry) (GameDbEntry, error) {
	db, err := LoadDB()
	if err != nil {
		return GameDbEntry{}, err
	}
	for i, p := range entry.SavePaths {
		entry.SavePaths[i] = pathutil.ShrinkEnv(p)
	}
	replaced := false
	for i := range db.Entries {
		if db.Entries[i].ID == entry.ID {
			db.Entries[i] = entry
			replaced = true
			break
		}
	}
	if !replaced {
		db.Entries = append(db.Entries, entry)
	}
	return entry, SaveDB(&db)
}

// RemoveEntry 删除条目。内置条目删除后会在下次读取时重新出现。
func RemoveEntry(id string) error {
	db, err := LoadDB()
	if err != nil {
		return err
	}
	kept := db.Entries[:0]
	for _, e := range db.Entries {
		if e.ID != id {
			kept = append(kept, e)
		}
	}
	db.Entries = kept
	return SaveDB(&db)
}

// ExportDB 返回缩进 JSON 形式的完整数据库。
func ExportDB() (string, error) {
	db, err := LoadDB()
	if err != nil {
		return "", err
	}
	data, err := json.MarshalIndent(db, "", "  ")
	return string(data), err
}

// ImportDB 合并导入的数据库：新 ID 直接追加；已有 ID 只在任一方为用户条目时覆盖，内置条目保持原值。
func ImportDB(raw string) error {
	var imported GameDatabase
	if err := json.Unmarshal([]byte(raw), &imported); err != nil {
		return err
	}
	db, err := LoadDB()
	if err != nil {
		return err
	}
	for _, entry := range imported.Entries {
		found := false
		for i := range db.Entries {
			if db.Entries[i].ID == entry.ID {
				found = true
				if db.Entries[i].Source == "user" || entry.Source == "user" {
					db.Entries[i] = entry
				}
				break
			}
		}
		if !found {
			db.Entries = append(db.Entries, entry)
		}
	}
	return SaveDB(&db)
}
