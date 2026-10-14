// Package backup 实现存档的全量/增量备份、还原、变更检测、启动前同步与云端旧备份清理。
//
// 每次备份在本地 {data}/manifests/<game_id>.json 追加一条清单，
// 并在云端游戏目录写入 manifest.json 记录最近一次备份的文件列表。
package backup

import (
	"encoding/json"
	"errors"
	"io/fs"
	"os"
	"path/filepath"
	"sort"
	"sync"
	"time"

	"github.com/ZeroDevi1/gamesave-manager/internal/appdir"
)

// BackupType 是备份类型。
type BackupType string

const (
	BackupFull        BackupType = "full"
	BackupIncremental BackupType = "incremental"
)

// FileEntry 是清单中的一个存档文件。
type FileEntry struct {
	// RelativePath 是相对所属存档路径的正斜杠路径。
	RelativePath string    `json:"relative_path"`
	Size         uint64    `json:"size"`
	ModifiedTime time.Time `json:"modified_time"`
	SHA256       string    `json:"sha256"`
}

// BackupManifest 是一次备份的清单。
type BackupManifest struct {
	GameID     string     `json:"game_id"`
	BackupType BackupType `json:"backup_type"`
	Timestamp  time.Time  `json:"timestamp"`
	// Files 是备份时存档目录中的全部文件（增量备份也记录未变化的文件，已删除的文件不再出现）。
	Files []FileEntry `json:"files"`
	// TargetPath 是云端位置：全量为 ZIP 路径，增量为 incremental/<时间戳> 目录。
	TargetPath string `json:"target_path"`
	// ZipFile 是全量备份的云端 ZIP 路径；增量备份为 nil。
	ZipFile *string `json:"zip_file,omitempty"`
}

// tsLayout 是备份时间戳的文本格式（UTC），用于 ZIP 文件名、增量目录名与还原时匹配清单。
const tsLayout = "20060102_150405"

// FormatTimestamp 把时间格式化为备份时间戳文本。
func FormatTimestamp(t time.Time) string { return t.UTC().Format(tsLayout) }

// manifestMu 串行化本地清单文件的读-改-写。
var manifestMu sync.Mutex

func manifestPath(gameID string) (string, error) {
	dir, err := appdir.ManifestsDir()
	if err != nil {
		return "", err
	}
	return filepath.Join(dir, gameID+".json"), nil
}

func readManifests(p string) ([]BackupManifest, error) {
	data, err := os.ReadFile(p)
	if errors.Is(err, fs.ErrNotExist) {
		return []BackupManifest{}, nil
	}
	if err != nil {
		return nil, err
	}
	var list []BackupManifest
	if err := json.Unmarshal(data, &list); err != nil {
		return nil, err
	}
	if list == nil {
		list = []BackupManifest{}
	}
	return list, nil
}

func writeManifests(p string, list []BackupManifest) error {
	data, err := json.MarshalIndent(list, "", "  ")
	if err != nil {
		return err
	}
	return os.WriteFile(p, data, 0o644)
}

// LoadManifests 返回游戏的全部本地备份清单（按写入顺序）；没有记录时返回空切片。
func LoadManifests(gameID string) ([]BackupManifest, error) {
	p, err := manifestPath(gameID)
	if err != nil {
		return nil, err
	}
	manifestMu.Lock()
	defer manifestMu.Unlock()
	return readManifests(p)
}

// saveManifest 追加一条清单。
func saveManifest(m BackupManifest) error {
	p, err := manifestPath(m.GameID)
	if err != nil {
		return err
	}
	manifestMu.Lock()
	defer manifestMu.Unlock()
	list, err := readManifests(p)
	if err != nil {
		return err
	}
	return writeManifests(p, append(list, m))
}

// pruneManifests 只保留时间最新的 keep 条清单。
func pruneManifests(gameID string, keep int) error {
	p, err := manifestPath(gameID)
	if err != nil {
		return err
	}
	manifestMu.Lock()
	defer manifestMu.Unlock()
	list, err := readManifests(p)
	if err != nil || len(list) <= keep {
		return err
	}
	sort.SliceStable(list, func(i, j int) bool { return list[i].Timestamp.After(list[j].Timestamp) })
	return writeManifests(p, list[:keep])
}

// LatestManifest 返回时间最新的清单；没有记录时返回 nil。
func LatestManifest(gameID string) (*BackupManifest, error) {
	list, err := LoadManifests(gameID)
	if err != nil || len(list) == 0 {
		return nil, err
	}
	latest := list[0]
	for _, m := range list[1:] {
		if m.Timestamp.After(latest.Timestamp) {
			latest = m
		}
	}
	return &latest, nil
}

// fileIndex 把清单文件列表转为以相对路径为键的索引。
func fileIndex(m *BackupManifest) map[string]FileEntry {
	idx := map[string]FileEntry{}
	if m == nil {
		return idx
	}
	for _, f := range m.Files {
		idx[f.RelativePath] = f
	}
	return idx
}

// changed 判断文件相对上次记录是否有变化：修改时间或 SHA256 任一不同即视为变化，新文件也算变化。
func changed(last map[string]FileEntry, cur FileEntry) bool {
	prev, ok := last[cur.RelativePath]
	return !ok || !prev.ModifiedTime.Equal(cur.ModifiedTime) || prev.SHA256 != cur.SHA256
}
