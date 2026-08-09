package main

import (
	"fyne.io/fyne/v2"
	"fyne.io/fyne/v2/container"
	"fyne.io/fyne/v2/dialog"
	"fyne.io/fyne/v2/storage"
	"fyne.io/fyne/v2/widget"
)

func buildConvertPanel(bin string) fyne.CanvasObject {
	targetEntry := widget.NewEntry()
	targetEntry.SetPlaceHolder("./my_project  or  ./file.bu")

	folderBrowse := widget.NewButton("Folder…", func() {
		d := dialog.NewFolderOpen(func(uri fyne.ListableURI, err error) {
			if err != nil || uri == nil {
				return
			}
			targetEntry.SetText(uri.Path())
		}, fyne.CurrentApp().Driver().AllWindows()[0])
		d.Show()
	})

	fileBrowse := widget.NewButton("File…", func() {
		d := dialog.NewFileOpen(func(uc fyne.URIReadCloser, err error) {
			if err != nil || uc == nil {
				return
			}
			targetEntry.SetText(uc.URI().Path())
			uc.Close()
		}, fyne.CurrentApp().Driver().AllWindows()[0])
		d.SetFilter(storage.NewExtensionFileFilter([]string{".bu"}))
		d.Show()
	})

	targetBrowse := container.NewHBox(folderBrowse, fileBrowse)

	// Language dropdown — explicit list so Java is clearly visible
	langSelect := widget.NewSelect(
		[]string{"(from #lang directive)", "rs", "py", "c", "cpp", "go", "java"},
		nil,
	)
	langSelect.SetSelected("(from #lang directive)")

	// Output path (single-file mode only)
	outputEntry := widget.NewEntry()
	outputEntry.SetPlaceHolder("out.rs  (optional, single-file mode only)")

	out, scroll := consoleOutput()
	btn := runButton("Run convert")

	btn.OnTapped = func() {
		args := []string{"convert"}
		if t := targetEntry.Text; t != "" {
			args = append(args, t)
		}
		// Add language override if selected
		if lang := langSelect.Selected; lang != "(from #lang directive)" && lang != "" {
			args = append(args, "-e", lang)
		}
		// Add output file if specified
		if o := outputEntry.Text; o != "" {
			args = append(args, "-o", o)
		}
		runBullarchy(bin, out, btn, args...)
	}

	form := container.NewVBox(
		labeledField("Source path", container.NewBorder(nil, nil, nil, targetBrowse, targetEntry)),
		labeledField("Target language", langSelect),
		labeledField("Output file", outputEntry),
		infoLabel("Leave language on '(from #lang directive)' to use the project's declared language."),
		btn,
	)

	return container.NewVBox(
		infoLabel("Transpile a Bullang project folder or a single .bu file."),
		widget.NewSeparator(),
		form,
		widget.NewSeparator(),
		scroll,
	)
}
