// GameSave Manager：游戏存档云备份工具。
//
// Go 侧负责存档扫描、打包、云端存储与配置持久化，通过 mygo.Bind 暴露给 src/ 下的 React 前端；
// 前端调用的 TypeScript 客户端由 `mygo generate` 生成到 src/mygo.ts。
package main

import (
	"log"
	"log/slog"
	"os"

	"github.com/egoist/mygo"

	"github.com/ZeroDevi1/gamesave-manager/internal/appdir"
	"github.com/ZeroDevi1/gamesave-manager/internal/game"
)

// logoScheme 是封面缓存目录对应的自定义协议，前端以 logos://localhost/<文件名>
// （Windows 上为 http://logos.localhost/<文件名>）加载本地封面。
const logoScheme = "logos"

func main() {
	mygo.Bind(
		&AlistService{},
		&StorageService{},
		&GamesService{},
		&GameDbService{},
		&PcgwService{},
		&BackupService{},
		&ConfigService{},
		&AppService{},
	)
	game.OpenURL = mygo.Shell.OpenExternal

	// 自定义协议须在创建使用它的窗口之前注册
	if dir, err := appdir.LogosDir(); err != nil {
		slog.Error("无法创建封面缓存目录，本地封面将无法显示", "err", err)
	} else if err := mygo.Protocol.Handle(logoScheme, mygo.FileServer(os.DirFS(dir))); err != nil {
		slog.Error("注册封面协议失败", "err", err)
	}

	mygo.App.WhenReady(func() {
		mygo.NewWindow(mygo.WindowOptions{
			Title:     "GameSave Manager",
			Width:     1200,
			Height:    800,
			MinWidth:  900,
			MinHeight: 600,
			// 与 src/styles/liquid-glass.css 的 --lg-bg-base 一致，避免页面绘制前闪白
			BackgroundColor: "light-dark(#f3f3f3, #1c1c1c)",
			StateKey:        "main",
			URL:             "/",
		})
	})
	if err := mygo.App.Run(); err != nil {
		log.Fatal(err)
	}
}
