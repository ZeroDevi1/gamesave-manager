package pathutil

import (
	"io/fs"
	"path/filepath"
)

// LocalFile 是存档路径下的一个本地文件。
type LocalFile struct {
	// Path 是文件的物理路径。
	Path string
	// Rel 是相对所属存档路径的正斜杠路径；存档路径本身是文件时为其文件名。
	// 备份 ZIP 条目、增量上传路径与清单都以它为键。
	Rel  string
	Info fs.FileInfo
}

// WalkSaveFiles 依次解析每个存档路径模板，对其下的每个文件调用 fn。
//
// strict 为 true 时，遍历中遇到的读取错误会中止并返回；为 false 时跳过无法访问的条目。
// fn 返回的错误总是中止遍历并原样返回。
func WalkSaveFiles(templates []string, strict bool, fn func(LocalFile) error) error {
	for _, tpl := range templates {
		for _, root := range ResolveSavePaths(tpl) {
			if err := walkRoot(root, strict, fn); err != nil {
				return err
			}
		}
	}
	return nil
}

func walkRoot(root string, strict bool, fn func(LocalFile) error) error {
	return filepath.WalkDir(root, func(path string, d fs.DirEntry, err error) error {
		if err != nil {
			if strict {
				return err
			}
			// 非严格模式：跳过无法读取的目录或文件，继续遍历其余条目
			if d != nil && d.IsDir() {
				return fs.SkipDir
			}
			return nil
		}
		if d.IsDir() {
			return nil
		}
		info, err := d.Info()
		if err != nil {
			if strict {
				return err
			}
			return nil
		}
		var rel string
		if path == root {
			// 存档路径本身就是文件
			rel = filepath.Base(root)
		} else {
			r, err := filepath.Rel(root, path)
			if err != nil {
				if strict {
					return err
				}
				return nil
			}
			rel = filepath.ToSlash(r)
		}
		return fn(LocalFile{Path: path, Rel: rel, Info: info})
	})
}
