package config

import (
	"encoding/json"
	"errors"
	"io/fs"
	"log/slog"
	"os"
	"path/filepath"
	"sync"

	"github.com/ZeroDevi1/gamesave-manager/internal/appdir"
)

// fileMu 串行化 config.json 的读写，避免并发命令交错写出半截文件。
// 它不保证“读-改-写”序列的原子性：两个命令同时修改配置时，后写入者覆盖先写入者。
var fileMu sync.Mutex

func path() (string, error) {
	dir, err := appdir.Dir()
	if err != nil {
		return "", err
	}
	return filepath.Join(dir, "config.json"), nil
}

// Load 读取 config.json；文件不存在时返回 Default()。
//
// 旧版只有 alist 字段而没有 storage 时，会迁移为 StorageConfig 并立即写回；
// 写回失败只记录日志，不影响本次读取结果。
func Load() (AppConfig, error) {
	p, err := path()
	if err != nil {
		return AppConfig{}, err
	}
	fileMu.Lock()
	data, err := os.ReadFile(p)
	fileMu.Unlock()
	if errors.Is(err, fs.ErrNotExist) {
		return Default(), nil
	}
	if err != nil {
		return AppConfig{}, err
	}

	var cfg AppConfig
	if err := json.Unmarshal(data, &cfg); err != nil {
		return AppConfig{}, err
	}
	if cfg.Games == nil {
		cfg.Games = []GameConfig{}
	}

	if cfg.Storage == nil && cfg.Alist != nil {
		s := cfg.Alist.toStorage()
		cfg.Storage = &s
		cfg.Alist = nil
		if err := Save(&cfg); err != nil {
			slog.Warn("[config] 迁移旧版 Alist 配置后写回失败", "err", err)
		}
	}
	return cfg, nil
}

// Save 以缩进 JSON 覆盖写入 config.json。
func Save(cfg *AppConfig) error {
	p, err := path()
	if err != nil {
		return err
	}
	if cfg.Games == nil {
		cfg.Games = []GameConfig{}
	}
	data, err := json.MarshalIndent(cfg, "", "  ")
	if err != nil {
		return err
	}
	fileMu.Lock()
	defer fileMu.Unlock()
	return os.WriteFile(p, data, 0o644)
}
