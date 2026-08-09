package main

import (
	"fyne.io/fyne/v2"
	"fyne.io/fyne/v2/container"
	"fyne.io/fyne/v2/widget"
)

func buildPackagesPanel(bin string) fyne.CanvasObject {
	tabs := container.NewAppTabs(
		container.NewTabItem("📦 install", buildInstallTab(bin)),
		container.NewTabItem("🗑 remove", buildRemoveTab(bin)),
		container.NewTabItem("📋 browse", buildBrowseTab(bin)),
	)
	tabs.SetTabLocation(container.TabLocationTop)
	return tabs
}

func buildInstallTab(bin string) fyne.CanvasObject {
	sourceEntry := widget.NewEntry()
	sourceEntry.SetPlaceHolder("package name  or  https://github.com/...")

	out, scroll := consoleOutput()
	btn := runButton("Install")

	btn.OnTapped = func() {
		src := sourceEntry.Text
		if src == "" {
			out.appendLine("✗ Enter a package name or URL.")
			return
		}
		runBullarchy(bin, out, btn, "add", src)
	}

	return container.NewVBox(
		infoLabel("Install a Bullang package from the registry or directly from a git URL."),
		widget.NewSeparator(),
		labeledField("Package", sourceEntry),
		btn,
		widget.NewSeparator(),
		scroll,
	)
}

func buildRemoveTab(bin string) fyne.CanvasObject {
	nameEntry := widget.NewEntry()
	nameEntry.SetPlaceHolder("package name")

	out, scroll := consoleOutput()
	btn := widget.NewButton("Remove", nil)
	btn.Importance = widget.DangerImportance

	btn.OnTapped = func() {
		name := nameEntry.Text
		if name == "" {
			out.appendLine("✗ Enter a package name.")
			return
		}
		runBullarchy(bin, out, btn, "remove", name)
	}

	return container.NewVBox(
		infoLabel("Uninstall a Bullang package. Feature libraries trigger a Bullarchy rebuild."),
		widget.NewSeparator(),
		labeledField("Package name", nameEntry),
		btn,
		widget.NewSeparator(),
		scroll,
	)
}

func buildBrowseTab(bin string) fyne.CanvasObject {
	out, scroll := consoleOutput()
	btn := runButton("Fetch registry")

	btn.OnTapped = func() {
		runBullarchy(bin, out, btn, "add")
	}

	return container.NewVBox(
		infoLabel("Browse all available packages from the Bullarchy registry."),
		widget.NewSeparator(),
		btn,
		widget.NewSeparator(),
		scroll,
	)
}
