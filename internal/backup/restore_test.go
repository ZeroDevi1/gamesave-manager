package backup

import (
	"archive/zip"
	"os"
	"path/filepath"
	"testing"
)

// 目录型存档：ZIP 条目相对存档目录本身，还原必须写回该目录，而不是其父目录。
func TestZipRoundTripRestoresIntoSaveDir(t *testing.T) {
	root := t.TempDir()
	saveDir := filepath.Join(root, "EldenRing")
	mustWrite(t, filepath.Join(saveDir, "76561198", "ER0000.sl2"), "slot")
	mustWrite(t, filepath.Join(saveDir, "settings.ini"), "cfg")

	zipPath := filepath.Join(root, "backup.zip")
	files, err := writeZip(zipPath, []string{saveDir})
	if err != nil {
		t.Fatal(err)
	}
	if len(files) != 2 {
		t.Fatalf("want 2 entries, got %d", len(files))
	}

	// 模拟本地存档被破坏后还原
	mustWrite(t, filepath.Join(saveDir, "76561198", "ER0000.sl2"), "corrupted")
	zr, err := zip.OpenReader(zipPath)
	if err != nil {
		t.Fatal(err)
	}
	defer zr.Close()
	target, err := extractTarget(saveDir, zr.File)
	if err != nil {
		t.Fatal(err)
	}
	if target != saveDir {
		t.Fatalf("target = %s, want %s", target, saveDir)
	}
	if err := extractAll(zr.File, target); err != nil {
		t.Fatal(err)
	}
	if got := mustRead(t, filepath.Join(saveDir, "76561198", "ER0000.sl2")); got != "slot" {
		t.Fatalf("restored content = %q", got)
	}
	if _, err := os.Stat(filepath.Join(root, "76561198")); !os.IsNotExist(err) {
		t.Fatal("files must not be extracted into the parent directory")
	}
}

// 文件型存档：条目名是文件名，还原到文件所在目录。
func TestExtractTargetForSingleFile(t *testing.T) {
	root := t.TempDir()
	save := filepath.Join(root, "save.dat")
	mustWrite(t, save, "x")
	target, err := extractTarget(save, nil)
	if err != nil {
		t.Fatal(err)
	}
	if target != root {
		t.Fatalf("target = %s, want %s", target, root)
	}
}

func TestExtractAllRejectsEscapingEntries(t *testing.T) {
	root := t.TempDir()
	zipPath := filepath.Join(root, "evil.zip")
	f, _ := os.Create(zipPath)
	zw := zip.NewWriter(f)
	w, _ := zw.Create("../escape.txt")
	w.Write([]byte("x"))
	zw.Close()
	f.Close()

	zr, err := zip.OpenReader(zipPath)
	if err != nil {
		t.Fatal(err)
	}
	defer zr.Close()
	if err := extractAll(zr.File, filepath.Join(root, "out")); err == nil {
		t.Fatal("expected error for entry outside target")
	}
}

func mustWrite(t *testing.T, p, content string) {
	t.Helper()
	if err := os.MkdirAll(filepath.Dir(p), 0o755); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(p, []byte(content), 0o644); err != nil {
		t.Fatal(err)
	}
}

func mustRead(t *testing.T, p string) string {
	t.Helper()
	b, err := os.ReadFile(p)
	if err != nil {
		t.Fatal(err)
	}
	return string(b)
}
