package main

import (
	_ "embed"
	"fmt"
	"os"
	"os/exec"
	"path/filepath"
	"runtime"
	"strings"

	"fyne.io/fyne/v2"
	"fyne.io/fyne/v2/app"
	"fyne.io/fyne/v2/canvas"
	"fyne.io/fyne/v2/container"
	"fyne.io/fyne/v2/widget"
)

//go:embed Icon.png
var iconBytes []byte

// ── OS detection ──────────────────────────────────────────────────────────────

type OS int

const (
	OSLinuxApt OS = iota
	OSLinuxArch
	OSMac
	OSWindows
	OSUnknown
)

func detectOS() OS {
	switch runtime.GOOS {
	case "windows":
		return OSWindows
	case "darwin":
		return OSMac
	case "linux":
		// Detect by which package manager is actually present, not by
		// matching distribution names in /etc/os-release. That test claimed
		// apt for Fedora, openSUSE and everything else it did not recognise,
		// so the installer ran commands that did not exist on the machine.
		if _, err := exec.LookPath("pacman"); err == nil {
			return OSLinuxArch
		}
		if _, err := exec.LookPath("apt-get"); err == nil {
			return OSLinuxApt
		}
		return OSUnknown
	default:
		return OSUnknown
	}
}

// ── Installation steps ────────────────────────────────────────────────────────

type Step struct {
	label string
	run   func(log func(string)) error
}

const bullarchyRepo = "https://github.com/The-Bullang-Foundation/Bullarchy.git"

func buildSteps(currentOS OS) []Step {
	return []Step{
		{
			label: "Installing Go...",
			run: func(log func(string)) error {
				if _, err := exec.LookPath("go"); err == nil {
					log("Go is already installed, skipping.")
					return nil
				}
				switch currentOS {
				case OSLinuxApt:
					_ = priv(log, "apt-get", "update", "-y")
					return priv(log, "apt-get", "install", "-y", "golang")
				case OSLinuxArch:
					return priv(log, "pacman", "-Sy", "--noconfirm", "go")
				case OSMac:
					if err := ensureBrew(log); err != nil {
						return err
					}
					return runCmd(log, "brew", "install", "go")
				case OSWindows:
					return runCmd(log, "winget", "install", "-e", "--id", "GoLang.Go", "--silent")
				default:
					return fmt.Errorf("unsupported OS")
				}
			},
		},
		{
			label: "Installing Rust and Cargo via rustup...",
			run: func(log func(string)) error {
				if _, err := exec.LookPath("cargo"); err == nil {
					log("Cargo is already installed, skipping.")
					return nil
				}
				switch currentOS {
				case OSWindows:
					return runCmd(log, "powershell", "-Command",
						"Invoke-WebRequest -Uri https://win.rustup.rs/x86_64 -OutFile $env:TEMP\\rustup-init.exe; "+
							"Start-Process -Wait -FilePath $env:TEMP\\rustup-init.exe -ArgumentList '-y'")
				default:
					return runCmd(log, "sh", "-c",
						"curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y")
				}
			},
		},
		{
			label: "Updating PATH for Cargo...",
			run: func(log func(string)) error {
				home, err := os.UserHomeDir()
				if err != nil {
					return fmt.Errorf("could not locate your home directory: %w", err)
				}
				// filepath.Join, not "+ \"/\"" — the separator is \\ on
				// Windows, which this step also runs on.
				cargoPath := filepath.Join(home, ".cargo", "bin")
				_ = os.Setenv("PATH", cargoPath+string(os.PathListSeparator)+os.Getenv("PATH"))
				// This only affects the installer's own child processes: a GUI
				// cannot change the PATH of the shell that will be used later.
				log("Added " + cargoPath + " to PATH for this session.")
				log("To make it permanent, add it to your shell profile.")
				return nil
			},
		},
		{
			label: "Installing Bullscript...",
			run: func(log func(string)) error {
				return runCmd(log, "cargo", "install",
					"--git", "https://github.com/The-Bullang-Foundation/Bullscript.git",
					"--force")
			},
		},
		{
			label: "Installing Bullarchy CLI...",
			run: func(log func(string)) error {
				return runCmd(log, "cargo", "install",
					"--git", bullarchyRepo,
					"--force")
			},
		},
		{
			label: "Cloning Bullarchy for GUI build...",
			run: func(log func(string)) error {
				home, _ := os.UserHomeDir()
				cloneDir := filepath.Join(home, ".bull", "bullarchy-src")
				// Remove old clone if present
				_ = os.RemoveAll(cloneDir)
				return runCmd(log, "git", "clone", "--depth", "1", bullarchyRepo, cloneDir)
			},
		},
		{
			label: "Installing GUI build dependencies...",
			run: func(log func(string)) error {
				switch currentOS {
				case OSLinuxApt:
					_ = priv(log, "apt-get", "update", "-y")
					return priv(log, "apt-get", "install", "-y",
						"gcc",
						"libgl1-mesa-dev",
						"libx11-dev",
						"libxrandr-dev",
						"libxinerama-dev",
						"libxcursor-dev",
						"libxi-dev",
						"libxxf86vm-dev")
				case OSLinuxArch:
					return priv(log, "pacman", "-Sy", "--noconfirm",
						"gcc",
						"mesa",
						"libx11",
						"libxrandr",
						"libxinerama",
						"libxcursor",
						"libxi")
				case OSMac:
					// Xcode command line tools provide everything needed
					log("macOS: Xcode tools provide GL/X11 equivalents via Metal/Fyne.")
					return nil
				case OSWindows:
					// Windows: no extra deps needed for Fyne
					log("Windows: no extra GUI dependencies needed.")
					return nil
				default:
					return nil
				}
			},
		},
		{
			label: "Building Bullarchy GUI...",
			run: func(log func(string)) error {
				home, _ := os.UserHomeDir()
				guiSrc := filepath.Join(home, ".bull", "bullarchy-src", "gui")
				guiBin := guiBinaryPath(home, currentOS)

				// Ensure output directory exists
				_ = os.MkdirAll(filepath.Dir(guiBin), 0755)

				// go mod tidy first
				tidyCmd := exec.Command("go", "mod", "tidy")
				tidyCmd.Dir = guiSrc
				tidyCmd.Stdout = &logWriter{log: log}
				tidyCmd.Stderr = &logWriter{log: log}
				log("$ go mod tidy")
				_ = tidyCmd.Run()

				// Build
				ldflags := "-s -w"
				if currentOS == OSWindows {
					ldflags = "-s -w -H=windowsgui"
				}
				buildCmd := exec.Command("go", "build", "-ldflags="+ldflags, "-o", guiBin, ".")
				buildCmd.Dir = guiSrc
				buildCmd.Stdout = &logWriter{log: log}
				buildCmd.Stderr = &logWriter{log: log}
				log(fmt.Sprintf("$ go build -o %s .", guiBin))
				return buildCmd.Run()
			},
		},
		{
			label: "Creating desktop shortcut for Bullarchy GUI...",
			run: func(log func(string)) error {
				home, _ := os.UserHomeDir()
				guiBin := guiBinaryPath(home, currentOS)
				return createDesktopShortcut(log, guiBin, currentOS, home)
			},
		},
		{
			label: "Running editor setup...",
			run: func(log func(string)) error {
				home, _ := os.UserHomeDir()
				return runCmd(log, home+"/.cargo/bin/bullarchy", "editor-setup")
			},
		},
		{
			label: "Installing target languages (Python, Java, C/C++)...",
			run: func(log func(string)) error {
				switch currentOS {
				case OSLinuxApt:
					_ = priv(log, "apt-get", "update", "-y")
					return priv(log, "apt-get", "install", "-y",
						"python3", "python3-pip", "default-jdk", "gcc", "g++", "build-essential")
				case OSLinuxArch:
					return priv(log, "pacman", "-Sy", "--noconfirm",
						"python", "python-pip", "jdk-openjdk", "gcc")
				case OSMac:
					if err := ensureBrew(log); err != nil {
						return err
					}
					return runCmd(log, "brew", "install", "python", "openjdk", "gcc")
				case OSWindows:
					for _, cmd := range [][]string{
						{"winget", "install", "-e", "--id", "Python.Python.3", "--silent"},
						{"winget", "install", "-e", "--id", "Microsoft.OpenJDK.21", "--silent"},
						{"winget", "install", "-e", "--id", "GnuWin32.Gcc", "--silent"},
					} {
						if err := runCmd(log, cmd[0], cmd[1:]...); err != nil {
							log(fmt.Sprintf("Warning: %v", err))
						}
					}
					return nil
				default:
					return fmt.Errorf("unsupported OS")
				}
			},
		},
	}
}

// ── Platform-specific helpers ─────────────────────────────────────────────────

func guiBinaryPath(home string, currentOS OS) string {
	switch currentOS {
	case OSWindows:
		return filepath.Join(home, "AppData", "Local", "Programs", "bullarchy-gui.exe")
	case OSMac:
		return "/usr/local/bin/bullarchy-gui"
	default: // Linux
		return filepath.Join(home, ".local", "bin", "bullarchy-gui")
	}
}

func createDesktopShortcut(log func(string), guiBin string, currentOS OS, home string) error {
	switch currentOS {
	case OSLinuxApt, OSLinuxArch:
		// Write a .desktop file
		desktopDir := filepath.Join(home, ".local", "share", "applications")
		_ = os.MkdirAll(desktopDir, 0755)
		desktopFile := filepath.Join(desktopDir, "bullarchy-gui.desktop")
		content := fmt.Sprintf(`[Desktop Entry]
Name=Bullarchy
Comment=Bullang project toolchain
Exec=%s
Icon=%s
Terminal=false
Type=Application
Categories=Development;
`, guiBin, guiBin)
		if err := os.WriteFile(desktopFile, []byte(content), 0644); err != nil {
			return err
		}
		log(fmt.Sprintf("Desktop shortcut created: %s", desktopFile))
		// Also add ~/.local/bin to PATH hint
		log("  Tip: add ~/.local/bin to your PATH if not already present.")
		return nil

	case OSMac:
		// Create a .app bundle in /Applications
		appDir := "/Applications/Bullarchy.app/Contents/MacOS"
		_ = os.MkdirAll(appDir, 0755)
		// Symlink the binary
		target := filepath.Join(appDir, "bullarchy-gui")
		_ = os.Remove(target)
		if err := os.Symlink(guiBin, target); err != nil {
			log(fmt.Sprintf("Warning: could not create .app symlink: %v", err))
		}
		// Write Info.plist
		plist := `<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>CFBundleName</key><string>Bullarchy</string>
  <key>CFBundleExecutable</key><string>bullarchy-gui</string>
  <key>CFBundleIdentifier</key><string>org.bullang.bullarchy</string>
  <key>CFBundleVersion</key><string>1.0</string>
  <key>CFBundlePackageType</key><string>APPL</string>
  <key>LSMinimumSystemVersion</key><string>10.13</string>
</dict>
</plist>`
		plistPath := "/Applications/Bullarchy.app/Contents/Info.plist"
		_ = os.MkdirAll(filepath.Dir(plistPath), 0755)
		if err := os.WriteFile(plistPath, []byte(plist), 0644); err != nil {
			log(fmt.Sprintf("Warning: could not write Info.plist: %v", err))
		}
		log("Bullarchy.app created in /Applications.")
		return nil

	case OSWindows:
		// Create a Start Menu shortcut via PowerShell
		shortcutPath := filepath.Join(home, "AppData", "Roaming", "Microsoft", "Windows", "Start Menu", "Programs", "Bullarchy.lnk")
		ps := fmt.Sprintf(`$s=(New-Object -COM WScript.Shell).CreateShortcut('%s');$s.TargetPath='%s';$s.Save()`,
			shortcutPath, guiBin)
		return runCmd(log, "powershell", "-Command", ps)

	default:
		log("Desktop shortcut creation skipped for unknown OS.")
		return nil
	}
}

// ── Helpers ───────────────────────────────────────────────────────────────────

func ensureBrew(log func(string)) error {
	if _, err := exec.LookPath("brew"); err == nil {
		return nil
	}
	log("Installing Homebrew...")
	return runCmd(log, "bash", "-c",
		`/bin/bash -c "$(curl -fsSL https://raw.githubusercontent.com/Homebrew/install/HEAD/install.sh)"`)
}

func privExec() string {
	for _, tool := range []string{"sudo", "doas", "pkexec"} {
		if _, err := exec.LookPath(tool); err == nil {
			return tool
		}
	}
	return ""
}

func priv(log func(string), args ...string) error {
	tool := privExec()
	if tool == "" {
		log("✗ No privilege escalation tool found (sudo/doas/pkexec).")
		log("  Please install the following manually: " + strings.Join(args, " "))
		return fmt.Errorf("no privilege escalation tool available")
	}
	return runCmd(log, tool, args...)
}

func runCmd(log func(string), name string, args ...string) error {
	log(fmt.Sprintf("$ %s %s", name, strings.Join(args, " ")))
	cmd := exec.Command(name, args...)
	cmd.Stdout = &logWriter{log: log}
	cmd.Stderr = &logWriter{log: log}
	return cmd.Run()
}

type logWriter struct{ log func(string) }

func (w *logWriter) Write(p []byte) (n int, err error) {
	for _, line := range strings.Split(strings.TrimRight(string(p), "\n"), "\n") {
		if strings.TrimSpace(line) != "" {
			w.log(line)
		}
	}
	return len(p), nil
}

// ── Result channel ────────────────────────────────────────────────────────────

type result struct {
	err      error
	stepName string
	done     bool
}

// ── UI ────────────────────────────────────────────────────────────────────────

func main() {
	currentOS := detectOS()
	a := app.New()
	a.SetIcon(fyne.NewStaticResource("Icon.png", iconBytes))
	w := a.NewWindow("Bullang Installer")
	w.Resize(fyne.NewSize(660, 420))
	w.SetFixedSize(true)

	logo := canvas.NewImageFromResource(fyne.NewStaticResource("Icon.png", iconBytes))
	logo.FillMode = canvas.ImageFillContain
	logo.SetMinSize(fyne.NewSize(96, 96))

	title := widget.NewLabelWithStyle("Bullang Ecosystem Installer",
		fyne.TextAlignCenter, fyne.TextStyle{Bold: true})
	subtitle := widget.NewLabelWithStyle(
		"Installs Go, Rust, Bullscript, Bullarchy CLI + GUI, and target languages",
		fyne.TextAlignCenter, fyne.TextStyle{Italic: true})

	osNames := map[OS]string{
		OSLinuxApt: "Ubuntu / Debian", OSLinuxArch: "Arch Linux",
		OSMac: "macOS", OSWindows: "Windows", OSUnknown: "Unknown OS",
	}
	osLabel := widget.NewLabelWithStyle(
		fmt.Sprintf("Detected OS: %s", osNames[currentOS]),
		fyne.TextAlignCenter, fyne.TextStyle{})

	progress := widget.NewProgressBar()
	progress.Hide()
	stepLabel := widget.NewLabel("")
	stepLabel.Alignment = fyne.TextAlignCenter
	stepLabel.Hide()

	errorLabel := widget.NewLabelWithStyle("", fyne.TextAlignCenter, fyne.TextStyle{Bold: true})
	errorLabel.Hide()
	errorLabel.Wrapping = fyne.TextWrapWord

	logOutput := widget.NewMultiLineEntry()
	logOutput.Disable()
	logOutput.Hide()
	logScroll := container.NewScroll(logOutput)
	logScroll.SetMinSize(fyne.NewSize(620, 120))

	results := make(chan result, 1)

	// Called from the install goroutine, so every widget touch goes through
	// runOnMain — see runner.go. On Fyne 2.5 that calls straight through, as
	// this always did; on 2.6 it becomes real main-thread dispatch, which 2.6
	// requires and panics without.
	appendLog := func(msg string) {
		runOnMain(func() {
			t := logOutput.Text
			if t == "" {
				logOutput.SetText(msg)
			} else {
				logOutput.SetText(t + "\n" + msg)
			}
			logScroll.ScrollToBottom()
		})
	}

	retryBtn := widget.NewButton("  Retry  ", nil)
	retryBtn.Hide()

	installBtn := widget.NewButton("  Install  ", nil)
	installBtn.Importance = widget.HighImportance

	installBtn.OnTapped = func() {
		installBtn.Disable()
		retryBtn.Hide()
		errorLabel.Hide()
		progress.Show()
		stepLabel.Show()
		logOutput.Show()
		logOutput.SetText("")

		steps := buildSteps(currentOS)
		total := float64(len(steps))

		go func() {
			for i, step := range steps {
				label := step.label
				runOnMain(func() {
					stepLabel.SetText(label)
					progress.SetValue(float64(i) / total)
				})
				appendLog(fmt.Sprintf("\n── %s", label))
				if err := step.run(appendLog); err != nil {
					appendLog(fmt.Sprintf("\n✗ FAILED: %v", err))
					results <- result{err: err, stepName: step.label}
					return
				}
			}
			results <- result{done: true}
		}()
	}

	retryBtn.OnTapped = func() { installBtn.OnTapped() }

	go func() {
		for r := range results {
			if r.err != nil {
				rr := r
				runOnMain(func() {
					errorLabel.SetText(fmt.Sprintf(
						"✗ Step failed: %s\n\nError: %v\n\nCheck the log above for details.",
						rr.stepName, rr.err))
					errorLabel.Show()
					progress.Hide()
					stepLabel.Hide()
					installBtn.Enable()
					retryBtn.Show()
					w.Canvas().Refresh(w.Content())
				})
			} else if r.done {
				runOnMain(func() {
					progress.SetValue(1)
					stepLabel.SetText("✓ Installation complete!")
				})
				appendLog("\n✓ Bullang ecosystem installed successfully.")
				appendLog("  Bullarchy GUI shortcut created — find it in your applications menu.")
				appendLog("  Restart your terminal, then run: bullarchy  or  bullscript")
				runOnMain(func() { w.Canvas().Refresh(w.Content()) })
			}
		}
	}()

	w.SetContent(container.NewPadded(container.NewVBox(
		container.NewCenter(logo),
		title, subtitle, osLabel,
		widget.NewSeparator(),
		container.NewCenter(installBtn),
		container.NewCenter(retryBtn),
		stepLabel, progress,
		errorLabel,
		widget.NewSeparator(),
		logScroll,
	)))
	w.ShowAndRun()
}
