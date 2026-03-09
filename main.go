package main

import (
	"embed"
	"log"

	"github.com/wailsapp/wails/v2"
	"github.com/wailsapp/wails/v2/pkg/menu"
	"github.com/wailsapp/wails/v2/pkg/menu/keys"
	"github.com/wailsapp/wails/v2/pkg/options"
	"github.com/wailsapp/wails/v2/pkg/options/assetserver"
	"github.com/wailsapp/wails/v2/pkg/options/mac"
)

//go:embed all:frontend/dist
var assets embed.FS

func main() {
	app := NewApp()

	appMenu := menu.NewMenu()
	appMenu.Append(menu.AppMenu())
	appMenu.Append(menu.EditMenu())

	recordMenu := appMenu.AddSubmenu("Recording")
	recordMenu.Append(menu.Text("Toggle Recording", keys.Combo("r", keys.ControlKey, keys.OptionOrAltKey), func(_ *menu.CallbackData) {
		app.toggleRecording()
	}))
	recordMenu.Append(menu.Separator())
	recordMenu.Append(menu.Text("Show Settings", nil, func(_ *menu.CallbackData) {
		app.showSettings()
	}))
	recordMenu.Append(menu.Text("Quit", keys.CmdOrCtrl("q"), func(_ *menu.CallbackData) {
		app.quit()
	}))

	if err := wails.Run(&options.App{
		Title:             "VoiceBox",
		Width:             700,
		Height:            450,
		MinWidth:          0,
		MinHeight:         0,
		Frameless:         true,
		StartHidden:       false,
		AlwaysOnTop:       false,
		HideWindowOnClose: true,
		BackgroundColour:  &options.RGBA{R: 0, G: 0, B: 0, A: 0},
		Menu:              appMenu,
		AssetServer: &assetserver.Options{
			Assets: assets,
		},
		OnStartup:  app.startup,
		OnShutdown: app.shutdown,
		Bind: []interface{}{
			app,
		},
		Mac: &mac.Options{
			WebviewIsTransparent: true,
			About: &mac.AboutInfo{
				Title:   "VoiceBox",
				Message: "Voice to Text",
			},
		},
	}); err != nil {
		log.Fatal(err)
	}
}
