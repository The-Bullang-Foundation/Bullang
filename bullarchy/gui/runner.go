package main

import (
	"bytes"
	"fmt"
	"io"
	"os/exec"
	"strings"
	"sync"

	"fyne.io/fyne/v2"
	"fyne.io/fyne/v2/container"
	"fyne.io/fyne/v2/theme"
	"fyne.io/fyne/v2/widget"
)

// runOnMain runs f on the goroutine Fyne expects to own its widgets.
//
// Fyne gained `fyne.Do` in **2.6**, and this project pins **2.5.4** — which is
// what broke the build: `undefined: fyne.Do`. 2.5 has no main-thread dispatch
// at all, so on this version the call happens where it is, exactly as it did
// before. That is not a fix for the underlying race; it is the behaviour 2.5
// has always had, and 2.5 tolerates it (2.6 is the release that made
// cross-thread widget access an error and gave you `fyne.Do` to avoid it).
//
// Upgrading is two lines. In go.mod:
//
//	require fyne.io/fyne/v2 v2.6.0
//
// and here:
//
//	func runOnMain(f func()) { fyne.Do(f) }
//
// Every other Fyne API this file uses was checked against the 2.5.4 source and
// exists there, so this shim is the whole of the incompatibility.
func runOnMain(f func()) {
	f()
}

// consoleOutput creates a scrollable, read-only log widget + scroll container.
func consoleOutput() (*console, *container.Scroll) {
	entry := widget.NewMultiLineEntry()
	entry.Disable()
	entry.SetPlaceHolder("Output will appear here...")
	scroll := container.NewScroll(entry)
	scroll.SetMinSize(fyne.NewSize(0, 200))
	return &console{entry: entry}, scroll
}

// console accumulates a command's output and mirrors it into a Fyne widget.
//
// Three separate problems lived in what this replaces:
//
//   - Widgets were mutated straight from a goroutine, which is a data race.
//     Every such mutation now goes through `runOnMain`, so the day this
//     project moves to Fyne 2.6 the race is closed by changing one function
//     rather than hunting the call sites again.
//   - Appending was O(n²): each line read the whole text back out, allocated
//     a new string with the line appended, and set it again. A long `convert`
//     slowed to a crawl as its own output grew.
//   - The io.Writer treated every Write as a whole number of lines. A pipe
//     splits wherever it likes, so a line arriving in two chunks was shown as
//     two lines, and blank lines — paragraph breaks in every one of
//     Bullarchy's messages — were dropped entirely.
type console struct {
	entry *widget.Entry

	mu      sync.Mutex
	buf     strings.Builder
	partial []byte
}

func (c *console) reset() {
	c.mu.Lock()
	c.buf.Reset()
	c.partial = nil
	c.mu.Unlock()
	c.sync()
}

// appendLine adds one line of text.
func (c *console) appendLine(msg string) {
	c.mu.Lock()
	if c.buf.Len() > 0 {
		c.buf.WriteByte('\n')
	}
	c.buf.WriteString(msg)
	c.mu.Unlock()
	c.sync()
}

// Write implements io.Writer over a byte stream of unknown framing.
func (c *console) Write(p []byte) (int, error) {
	c.mu.Lock()
	c.partial = append(c.partial, p...)
	for {
		i := bytes.IndexByte(c.partial, '\n')
		if i < 0 {
			break
		}
		line := strings.TrimRight(string(c.partial[:i]), "\r")
		if c.buf.Len() > 0 {
			c.buf.WriteByte('\n')
		}
		// Blank lines are kept: they are how the CLI separates sections.
		c.buf.WriteString(line)
		c.partial = c.partial[i+1:]
	}
	c.mu.Unlock()
	c.sync()
	return len(p), nil
}

// flush emits a trailing line that never got its newline — a prompt, or the
// last line of a command that did not end with one.
func (c *console) flush() {
	c.mu.Lock()
	if len(c.partial) > 0 {
		if c.buf.Len() > 0 {
			c.buf.WriteByte('\n')
		}
		c.buf.Write(c.partial)
		c.partial = nil
	}
	c.mu.Unlock()
	c.sync()
}

// sync pushes the accumulated text to the widget on the main thread.
func (c *console) sync() {
	c.mu.Lock()
	text := c.buf.String()
	c.mu.Unlock()
	runOnMain(func() { c.entry.SetText(text) })
}

// runBullarchy runs `bullarchy <args...>` asynchronously, streaming output to
// `out`. Disables `btn` during execution and re-enables on completion.
func runBullarchy(bin string, out *console, btn *widget.Button, args ...string) {
	btn.Disable()
	out.reset()
	go func() {
		defer runOnMain(func() { btn.Enable() })

		out.appendLine(fmt.Sprintf("$ bullarchy %s\n", strings.Join(args, " ")))

		cmd := exec.Command(bin, args...)
		cmd.Stdout = out
		cmd.Stderr = out
		// Stdin was never set, so it inherited the GUI's — which has no
		// terminal attached. Anything that prompted blocked forever with no
		// indication why. An empty stdin gives the child an immediate EOF, so
		// it takes its own no-input path and reports whatever it needs.
		cmd.Stdin = strings.NewReader("")

		err := cmd.Run()
		out.flush()
		if err != nil {
			out.appendLine(fmt.Sprintf("\n✗ %v", err))
		} else {
			out.appendLine("\n✓ Done.")
		}
	}()
}

var _ io.Writer = (*console)(nil)

// labeledField returns a VBox with a label above a widget.
func labeledField(label string, w fyne.CanvasObject) *fyne.Container {
	return container.NewVBox(
		widget.NewLabelWithStyle(label, fyne.TextAlignLeading, fyne.TextStyle{Bold: true}),
		w,
	)
}

// runButton creates a styled primary action button.
func runButton(label string) *widget.Button {
	btn := widget.NewButtonWithIcon(label, theme.MediaPlayIcon(), nil)
	btn.Importance = widget.HighImportance
	return btn
}

// infoLabel creates an italic info/hint label.
func infoLabel(text string) *widget.Label {
	l := widget.NewLabelWithStyle(text, fyne.TextAlignLeading, fyne.TextStyle{Italic: true})
	l.Wrapping = fyne.TextWrapWord
	return l
}
