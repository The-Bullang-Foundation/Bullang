package main

import (
	"fyne.io/fyne/v2"
	"fyne.io/fyne/v2/container"
	"fyne.io/fyne/v2/widget"
)

func buildControlPanel(bin string) fyne.CanvasObject {
	tabs := container.NewAppTabs(
		container.NewTabItem("🔍 check", buildCheckTab(bin)),
		container.NewTabItem("✨ fmt", buildFmtTab(bin)),
	)
	tabs.SetTabLocation(container.TabLocationTop)
	return tabs
}

func buildCheckTab(bin string) fyne.CanvasObject {
	out, scroll := consoleOutput()
	btn := runButton("Run check")
	btn.OnTapped = func() {
		runBullarchy(bin, out, btn, "check")
	}
	return container.NewVBox(
		infoLabel("Validate structure, type-check, and verify formatting from the current directory."),
		widget.NewSeparator(),
		btn,
		widget.NewSeparator(),
		scroll,
	)
}

func buildFmtTab(bin string) fyne.CanvasObject {
	folderEntry := widget.NewEntry()
	folderEntry.SetPlaceHolder("(optional) folder path")

	dryRun := widget.NewCheck("Dry run (preview only, no writes)", nil)

	out, scroll := consoleOutput()
	btn := runButton("Run fmt")

	btn.OnTapped = func() {
		args := []string{"fmt"}
		if f := folderEntry.Text; f != "" {
			args = append(args, f)
		}
		if dryRun.Checked {
			args = append(args, "--dry-run")
		}
		runBullarchy(bin, out, btn, args...)
	}

	return container.NewVBox(
		infoLabel("Reformat all .bu files to canonical style."),
		widget.NewSeparator(),
		labeledField("Folder", folderEntry),
		dryRun,
		btn,
		widget.NewSeparator(),
		scroll,
	)
}
