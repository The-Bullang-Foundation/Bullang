package main

import (
	"fmt"
	"strconv"

	"fyne.io/fyne/v2"
	"fyne.io/fyne/v2/container"
	"fyne.io/fyne/v2/dialog"
	"fyne.io/fyne/v2/storage"
	"fyne.io/fyne/v2/widget"
)

var rankNames = []string{"skirmish", "tactic", "strategy", "battle", "theater", "war"}
var langOptions = []string{"(none)", "rs", "py", "c", "cpp", "go", "java"}

func buildInitPanel(bin string) fyne.CanvasObject {
	nameEntry := widget.NewEntry()
	nameEntry.SetPlaceHolder("my_project")

	pathEntry := widget.NewEntry()
	pathEntry.SetPlaceHolder("/home/user/projects  (optional)")

	pathBrowse := widget.NewButton("Browse", func() {
		// Open folder dialog via OS dialog
		d := dialog.NewFolderOpen(func(uri fyne.ListableURI, err error) {
			if err != nil || uri == nil {
				return
			}
			pathEntry.SetText(uri.Path())
		}, fyne.CurrentApp().Driver().AllWindows()[0])
		d.Show()
	})

	blueprintEntry := widget.NewEntry()
	blueprintEntry.SetPlaceHolder("(optional) path to blueprint.bu")

	blueprintBrowse := widget.NewButton("Browse", func() {
		d := dialog.NewFileOpen(func(uc fyne.URIReadCloser, err error) {
			if err != nil || uc == nil {
				return
			}
			blueprintEntry.SetText(uc.URI().Path())
			uc.Close()
		}, fyne.CurrentApp().Driver().AllWindows()[0])
		d.SetFilter(storage.NewExtensionFileFilter([]string{".bu"}))
		d.Show()
	})

	// Depth slider
	depthLabel := widget.NewLabel("2 — tactic")
	depthSlider := widget.NewSlider(1, 6)
	depthSlider.Value = 2
	depthSlider.Step = 1
	depthSlider.OnChanged = func(v float64) {
		i := int(v) - 1
		if i < 0 {
			i = 0
		}
		if i >= len(rankNames) {
			i = len(rankNames) - 1
		}
		depthLabel.SetText(fmt.Sprintf("%d — %s", int(v), rankNames[i]))
	}

	// Lang select
	langSelect := widget.NewSelect(langOptions, nil)
	langSelect.SetSelected("(none)")

	// Lib entries (up to 3)
	lib1 := widget.NewEntry()
	lib1.SetPlaceHolder("e.g. stdio.h (optional)")
	lib2 := widget.NewEntry()
	lib2.SetPlaceHolder("optional")
	lib3 := widget.NewEntry()
	lib3.SetPlaceHolder("optional")

	out, scroll := consoleOutput()
	btn := runButton("Run init")

	btn.OnTapped = func() {
		name := nameEntry.Text
		if name == "" {
			out.appendLine("✗ Project name is required.")
			return
		}
		args := []string{"init", name}

		depth := int(depthSlider.Value)
		if depth != 2 {
			args = append(args, "--depth", strconv.Itoa(depth))
		}
		if lang := langSelect.Selected; lang != "(none)" && lang != "" {
			args = append(args, "--lang", lang)
		}
		for _, lib := range []string{lib1.Text, lib2.Text, lib3.Text} {
			if lib != "" {
				args = append(args, "--lib", lib)
			}
		}
		if bp := blueprintEntry.Text; bp != "" {
			args = append(args, "--blueprint", bp)
		}
		if path := pathEntry.Text; path != "" {
			args = append(args, "--path", path)
		}
		runBullarchy(bin, out, btn, args...)
	}

	form := container.NewVBox(
		labeledField("Project name", nameEntry),
		labeledField("Output path", container.NewBorder(nil, nil, nil, pathBrowse, pathEntry)),
		labeledField("Blueprint", container.NewBorder(nil, nil, nil, blueprintBrowse, blueprintEntry)),
		labeledField("Depth", container.NewVBox(depthSlider, depthLabel)),
		labeledField("Language", langSelect),
		labeledField("Libraries", container.NewVBox(lib1, lib2, lib3)),
		btn,
	)

	return container.NewVBox(
		infoLabel("Scaffold a new Bullang project."),
		widget.NewSeparator(),
		form,
		widget.NewSeparator(),
		scroll,
	)
}
