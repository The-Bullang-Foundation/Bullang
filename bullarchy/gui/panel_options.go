package main

import (
	"fyne.io/fyne/v2"
	"fyne.io/fyne/v2/container"
	"fyne.io/fyne/v2/widget"
)

func buildOptionsPanel(bin string) fyne.CanvasObject {
	tabs := container.NewAppTabs(
		container.NewTabItem("🛠️ editor-setup", buildEditorSetupTab(bin)),
		container.NewTabItem("🚀 update", buildUpdateTab(bin)),
	)
	tabs.SetTabLocation(container.TabLocationTop)
	return tabs
}

func buildEditorSetupTab(bin string) fyne.CanvasObject {
	out, scroll := consoleOutput()
	btn := runButton("Run editor-setup")
	btn.OnTapped = func() {
		runBullarchy(bin, out, btn, "editor-setup")
	}
	return container.NewVBox(
		infoLabel("Write LSP configuration files for Vim, Neovim, Helix, and Emacs automatically.\nFor VSCode: install the extension from the Bullang repo."),
		widget.NewSeparator(),
		btn,
		widget.NewSeparator(),
		scroll,
	)
}

func buildUpdateTab(bin string) fyne.CanvasObject {
	out, scroll := consoleOutput()
	btn := runButton("Update all Bullang tools")

	btn.OnTapped = func() {
		runBullarchy(bin, out, btn, "update")
	}

	return container.NewVBox(
		infoLabel("Clears cargo caches and reinstalls Bullang, Bullarchy and Bullscript\nfrom their latest commits on main. May take a few minutes."),
		widget.NewSeparator(),
		btn,
		widget.NewSeparator(),
		scroll,
	)
}
