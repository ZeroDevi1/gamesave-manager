package backup

import (
	"context"
	"log/slog"

	"github.com/ZeroDevi1/gamesave-manager/internal/config"
	"github.com/ZeroDevi1/gamesave-manager/internal/pathutil"
)

// GameChange 是一个有未备份变更的游戏。
type GameChange struct {
	GameID       string `json:"game_id"`
	ChangedFiles int    `json:"changed_files"`
}

// BatchBackupResult 是批量备份中单个游戏的结果。
type BatchBackupResult struct {
	GameID  string `json:"game_id"`
	Success bool   `json:"success"`
	Message string `json:"message"`
}

// scanLocal 计算游戏当前全部存档文件的清单条目，跳过无法读取的文件。
func scanLocal(g *config.GameConfig) map[string]FileEntry {
	out := map[string]FileEntry{}
	_ = pathutil.WalkSaveFiles(g.SavePaths, false, func(f pathutil.LocalFile) error {
		sum, err := fileSHA256(f.Path)
		if err != nil {
			return nil
		}
		out[f.Rel] = entryOf(f, sum)
		return nil
	})
	return out
}

// CheckAllGamesForChanges 对比每个游戏的本地存档与最新清单，返回有变更（修改时间或 SHA256 不同、或新文件）的游戏。
func CheckAllGamesForChanges() ([]GameChange, error) {
	cfg, err := config.Load()
	if err != nil {
		return nil, err
	}
	out := []GameChange{}
	for i := range cfg.Games {
		g := &cfg.Games[i]
		last, err := LatestManifest(g.ID)
		if err != nil {
			return nil, err
		}
		lastFiles := fileIndex(last)
		n := 0
		for _, entry := range scanLocal(g) {
			if changed(lastFiles, entry) {
				n++
			}
		}
		if n > 0 {
			slog.Info("[变更检测] 发现变更", "game", g.Name, "files", n)
			out = append(out, GameChange{GameID: g.ID, ChangedFiles: n})
		}
	}
	slog.Info("[变更检测] 完成", "changedGames", len(out))
	return out, nil
}

// BackupAllChangedGames 对每个有变更的游戏执行增量备份；单个游戏失败不影响其它游戏，结果中逐个报告。
func BackupAllChangedGames(ctx context.Context) ([]BatchBackupResult, error) {
	changes, err := CheckAllGamesForChanges()
	if err != nil {
		return nil, err
	}
	out := []BatchBackupResult{}
	for _, c := range changes {
		slog.Info("[批量备份] 开始备份", "game", c.GameID, "changed", c.ChangedFiles)
		res, err := IncrementalBackup(ctx, c.GameID)
		if err != nil {
			slog.Error("[批量备份] 失败", "game", c.GameID, "err", err)
			out = append(out, BatchBackupResult{GameID: c.GameID, Success: false, Message: err.Error()})
			continue
		}
		out = append(out, BatchBackupResult{GameID: c.GameID, Success: true, Message: res.Message})
	}
	slog.Info("[批量备份] 完成", "count", len(out))
	return out, nil
}
