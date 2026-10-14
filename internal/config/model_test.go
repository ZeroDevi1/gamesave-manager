package config

import (
	"encoding/json"
	"testing"
)

// 前端保存配置时只回传它知道的设置字段，缺失的 auto_backup 等必须保持默认 true。
func TestSettingsDefaultsWhenFieldsMissing(t *testing.T) {
	var cfg AppConfig
	if err := json.Unmarshal([]byte(`{"games":[],"settings":{"theme":"dark"}}`), &cfg); err != nil {
		t.Fatal(err)
	}
	if cfg.Settings.Theme != "dark" || !cfg.Settings.AutoBackup || !cfg.Settings.RequireConfirmation {
		t.Fatalf("settings = %+v", cfg.Settings)
	}

	var noSettings AppConfig
	if err := json.Unmarshal([]byte(`{"games":[]}`), &noSettings); err != nil {
		t.Fatal(err)
	}
	if noSettings.Settings != DefaultSettings() {
		t.Fatalf("settings = %+v", noSettings.Settings)
	}
}

func TestGameRemotePath(t *testing.T) {
	g := &GameConfig{RemotePath: "/GameSaves/Kingdom Come: Deliverance II"}
	cfg := &AppConfig{}
	if got := cfg.GameRemotePath(g); got != "/GameSaves/Kingdom Come_ Deliverance II" {
		t.Errorf("no root: %q", got)
	}
	cfg.Storage = &StorageConfig{Type: StorageNetdisk, BackupRoot: "/CloudSave/"}
	if got := cfg.GameRemotePath(g); got != "/CloudSave/Kingdom Come_ Deliverance II" {
		t.Errorf("with root: %q", got)
	}
}

// 读取 Tauri 版写出的配置：Option 字段为 null，存储配置为带 type 标签的对象。
func TestReadTauriConfig(t *testing.T) {
	raw := `{"storage":{"type":"netdisk","driver":"quark","token":"c","refresh_token":null,"backup_root":null},
	"games":[{"id":"x","name":"X","save_paths":["a"],"remote_path":"/GameSaves/X","last_backup":null,"logo_path":null,"steam_appid":42}],
	"settings":{"theme":"system","steamgriddb_api_key":null,"auto_backup":false,"require_confirmation":true,"backup_retention_limit":3}}`
	var cfg AppConfig
	if err := json.Unmarshal([]byte(raw), &cfg); err != nil {
		t.Fatal(err)
	}
	if cfg.Storage.Driver != "quark" || cfg.Storage.Token != "c" || *cfg.Games[0].SteamAppID != 42 ||
		cfg.Settings.AutoBackup || *cfg.Settings.BackupRetentionLimit != 3 {
		t.Fatalf("cfg = %+v", cfg)
	}
}
