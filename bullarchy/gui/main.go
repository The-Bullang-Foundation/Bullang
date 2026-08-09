// Command bullarchy-gui is the graphical front end to the Bullarchy toolchain.
//
// It shows the toolchain: a tab per area of it, each shelling out to the
// `bullarchy` binary and streaming its output back into the window.
//
// It used to show an *installer* — a window titled "Bullang Installer" with a
// single Install button that installed Go, Rust, Bullscript and Bullarchy. That
// was circular (you cannot launch it before installing it), it duplicated the
// standalone bullang-installer repository, and it left the six panels beside
// this file — init, convert, control, packages, blueprint, options — as dead
// code: complete, compiling, and referenced by nothing. They are wired in here.
//
// Installing the ecosystem is the job of:
//
//	https://github.com/The-Bullang-Foundation/bullang-installer
//	https://github.com/The-Bullang-Foundation/bullang-installer-cli
package main

import (
	_ "embed"
	"fmt"
	"os"
	"os/exec"
	"path/filepath"
	"runtime"

	"fyne.io/fyne/v2"
	"fyne.io/fyne/v2/app"
	"fyne.io/fyne/v2/container"
	"fyne.io/fyne/v2/theme"
	"fyne.io/fyne/v2/widget"
)

//go:embed Icon.png
var iconBytes []byte

// findBullarchy locates the CLI this GUI drives.
//
// The same places, in the same order, that `bullarchy` itself searches for
// `bullarchy-gui` — so the pair find each other however they were installed.
func findBullarchy() (string, bool) {
	name := "bullarchy"
	if runtime.GOOS == "windows" {
		name = "bullarchy.exe"
	}

	var candidates []string

	// Beside this binary: how `bullarchy update` installs the two.
	if self, err := os.Executable(); err == nil {
		candidates = append(candidates, filepath.Join(filepath.Dir(self), name))
	}
	if home, err := os.UserHomeDir(); err == nil {
		candidates = append(candidates,
			filepath.Join(home, ".cargo", "bin", name), // cargo install's default
			filepath.Join(home, ".local", "bin", name),
			filepath.Join(home, "AppData", "Local", "Programs", name),
		)
	}
	candidates = append(candidates, filepath.Join("/usr", "local", "bin", name))

	for _, c := range candidates {
		if info, err := os.Stat(c); err == nil && !info.IsDir() {
			return c, true
		}
	}

	// Last resort: whatever PATH says.
	if p, err := exec.LookPath(name); err == nil {
		return p, true
	}
	return "", false
}

// missingBullarchy is shown instead of the tabs when the CLI cannot be found.
//
// Every panel works by running `bullarchy`, so without it the window would be a
// set of buttons that each fail the same way. Saying so once, with what to do
// about it, is more use than six identical errors.
func missingBullarchy() fyne.CanvasObject {
	// No word wrapping: a wrapping label has no natural width, and inside a
	// centring container that collapses it to one character per line. The
	// message carries its own line breaks instead.
	msg := widget.NewLabelWithStyle(
		"The bullarchy command could not be found.\n\n"+
			"This window drives it, so there is nothing it can do without it.",
		fyne.TextAlignCenter, fyne.TextStyle{Bold: true})

	how := widget.NewLabelWithStyle(
		"Install it with:", fyne.TextAlignCenter, fyne.TextStyle{})

	// Selectable and monospaced, so it can be copied rather than retyped.
	cmd := widget.NewRichTextWithText(
		"cargo install --git https://github.com/The-Bullang-Foundation/Bullang.git bullang bullarchy")
	cmd.Wrapping = fyne.TextWrapBreak

	after := widget.NewLabelWithStyle(
		"or run the graphical installer, then reopen this window.",
		fyne.TextAlignCenter, fyne.TextStyle{Italic: true})

	return container.NewPadded(container.NewVBox(
		widget.NewLabel(""), msg, how, cmd, after,
	))
}

func main() {
	a := app.New()
	a.SetIcon(fyne.NewStaticResource("Icon.png", iconBytes))

	w := a.NewWindow("Bullarchy")
	w.Resize(fyne.NewSize(900, 700))

	bin, found := findBullarchy()
	if !found {
		w.SetContent(missingBullarchy())
		w.ShowAndRun()
		return
	}

	// One tab per area of the toolchain. "Check & Format" carries check and
	// fmt, and "Options" carries editor-setup and update, because each of those
	// pairs is two fields and a button and does not earn a tab of its own.
	tabs := container.NewAppTabs(
		container.NewTabItemWithIcon("Init", theme.FolderNewIcon(), buildInitPanel(bin)),
		container.NewTabItemWithIcon("Convert", theme.MediaPlayIcon(), buildConvertPanel(bin)),
		container.NewTabItemWithIcon("Check & Format", theme.ConfirmIcon(), buildControlPanel(bin)),
		container.NewTabItemWithIcon("Packages", theme.StorageIcon(), buildPackagesPanel(bin)),
		container.NewTabItemWithIcon("Blueprint", theme.DocumentCreateIcon(), buildBlueprintPanel(bin)),
		container.NewTabItemWithIcon("Options", theme.SettingsIcon(), buildOptionsPanel(bin)),
	)
	tabs.SetTabLocation(container.TabLocationTop)

	status := widget.NewLabelWithStyle(
		fmt.Sprintf("using %s", bin),
		fyne.TextAlignTrailing, fyne.TextStyle{Italic: true})

	w.SetContent(container.NewBorder(nil, status, nil, nil, tabs))
	w.ShowAndRun()
}
