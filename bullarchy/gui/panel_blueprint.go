package main

import (
	"fmt"
	"os"
	"path/filepath"
	"strings"

	"fyne.io/fyne/v2"
	"fyne.io/fyne/v2/container"
	"fyne.io/fyne/v2/dialog"
	"fyne.io/fyne/v2/storage"
	"fyne.io/fyne/v2/widget"
)

// blueprintNode represents a folder or file in the project tree.
type blueprintNode struct {
	name     string
	lang     string // folder lang override
	rank     string // auto from depth
	isFile   bool
	bullets  []string // function names (files only)
	children []*blueprintNode
}

var bpRoot = &blueprintNode{name: "project", rank: "war"}

func buildBlueprintPanel(bin string) fyne.CanvasObject {
	// Tree display
	treeWidget := widget.NewTree(
		func(id widget.TreeNodeID) []widget.TreeNodeID {
			if id == "" {
				return []widget.TreeNodeID{"root"}
			}
			node := bpFindNode(id, bpRoot)
			if node == nil {
				return nil
			}
			ids := make([]widget.TreeNodeID, len(node.children))
			for i, c := range node.children {
				ids[i] = id + "/" + c.name
			}
			return ids
		},
		func(id widget.TreeNodeID) bool {
			node := bpFindNode(id, bpRoot)
			return node != nil && !node.isFile && len(node.children) > 0
		},
		func(branch bool) fyne.CanvasObject {
			return widget.NewLabel("")
		},
		func(id widget.TreeNodeID, branch bool, o fyne.CanvasObject) {
			node := bpFindNode(id, bpRoot)
			if node == nil {
				return
			}
			lbl := o.(*widget.Label)
			if node.isFile {
				lbl.SetText("📄 " + node.name + ".bu")
			} else {
				lbl.SetText("📁 " + node.name + " [" + node.rank + "]")
			}
		},
	)

	// Edit area
	nameEntry := widget.NewEntry()
	nameEntry.SetPlaceHolder("node name")
	langSelect := widget.NewSelect(langOptions, nil)
	langSelect.SetSelected("(none)")
	isFileCheck := widget.NewCheck("Is a .bu source file", nil)
	bulletsEntry := widget.NewEntry()
	bulletsEntry.SetPlaceHolder("function names, comma separated")
	bulletsEntry.Hide()

	isFileCheck.OnChanged = func(b bool) {
		if b {
			bulletsEntry.Show()
		} else {
			bulletsEntry.Hide()
		}
	}

	var selectedID widget.TreeNodeID

	treeWidget.OnSelected = func(id widget.TreeNodeID) {
		selectedID = id
		node := bpFindNode(id, bpRoot)
		if node == nil {
			return
		}
		nameEntry.SetText(node.name)
		langSelect.SetSelected(func() string {
			if node.lang == "" {
				return "(none)"
			}
			return node.lang
		}())
		isFileCheck.SetChecked(node.isFile)
		bulletsEntry.SetText(strings.Join(node.bullets, ", "))
	}

	addChildBtn := widget.NewButton("＋ Add child", func() {
		parent := bpFindNode(selectedID, bpRoot)
		if parent == nil || parent.isFile {
			parent = bpRoot
		}
		parent.children = append(parent.children, &blueprintNode{
			name: "new_folder",
			rank: bpRankForDepth(bpDepth(selectedID) + 1),
		})
		treeWidget.Refresh()
	})

	addFileBtn := widget.NewButton("＋ Add file", func() {
		parent := bpFindNode(selectedID, bpRoot)
		if parent == nil || parent.isFile {
			parent = bpRoot
		}
		parent.children = append(parent.children, &blueprintNode{
			name:   "new_file",
			isFile: true,
		})
		treeWidget.Refresh()
	})

	deleteBtn := widget.NewButton("🗑 Delete", func() {
		if selectedID == "" || selectedID == "root" {
			return
		}
		bpDeleteNode(selectedID, bpRoot)
		selectedID = ""
		treeWidget.Refresh()
	})

	applyBtn := widget.NewButton("Apply changes", func() {
		node := bpFindNode(selectedID, bpRoot)
		if node == nil {
			return
		}
		node.name = nameEntry.Text
		if l := langSelect.Selected; l != "(none)" {
			node.lang = l
		} else {
			node.lang = ""
		}
		node.isFile = isFileCheck.Checked
		if node.isFile {
			parts := strings.Split(bulletsEntry.Text, ",")
			node.bullets = nil
			for _, p := range parts {
				if t := strings.TrimSpace(p); t != "" {
					node.bullets = append(node.bullets, t)
				}
			}
		}
		treeWidget.Refresh()
	})

	// Save to disk
	saveEntry := widget.NewEntry()
	saveEntry.SetPlaceHolder("blueprint.bu")
	saveBrowse := widget.NewButton("Save as...", func() {
		d := dialog.NewFileSave(func(uc fyne.URIWriteCloser, err error) {
			if err != nil || uc == nil {
				return
			}
			content := bpSerialize(bpRoot, 0)
			uc.Write([]byte(content))
			uc.Close()
			saveEntry.SetText(uc.URI().Path())
		}, fyne.CurrentApp().Driver().AllWindows()[0])
		d.SetFilter(storage.NewExtensionFileFilter([]string{".bu"}))
		d.SetFileName("blueprint.bu")
		d.Show()
	})

	// Load existing blueprint
	loadBtn := widget.NewButton("Load blueprint.bu", func() {
		d := dialog.NewFileOpen(func(uc fyne.URIReadCloser, err error) {
			if err != nil || uc == nil {
				return
			}
			defer uc.Close()
			data, _ := os.ReadFile(uc.URI().Path())
			bpRoot = bpParse(string(data))
			treeWidget.Refresh()
		}, fyne.CurrentApp().Driver().AllWindows()[0])
		d.SetFilter(storage.NewExtensionFileFilter([]string{".bu"}))
		d.Show()
	})

	editPanel := container.NewVBox(
		labeledField("Name", nameEntry),
		labeledField("Language override", langSelect),
		isFileCheck,
		labeledField("Bullets", bulletsEntry),
		container.NewGridWithColumns(2, applyBtn, deleteBtn),
		widget.NewSeparator(),
		container.NewGridWithColumns(2, addChildBtn, addFileBtn),
	)

	savePanel := container.NewVBox(
		widget.NewSeparator(),
		container.NewBorder(nil, nil, nil, saveBrowse, saveEntry),
		loadBtn,
	)

	left := container.NewBorder(
		infoLabel("Click a node to edit it."), savePanel, nil, nil,
		treeWidget,
	)

	return container.NewHSplit(
		left,
		container.NewVBox(
			infoLabel("Edit selected node:"),
			widget.NewSeparator(),
			editPanel,
		),
	)
}

// ── Blueprint tree helpers ─────────────────────────────────────────────────────

func bpFindNode(id widget.TreeNodeID, root *blueprintNode) *blueprintNode {
	if id == "root" || id == "" {
		return root
	}
	parts := strings.SplitN(id, "/", 2)
	if parts[0] != "root" {
		return nil
	}
	if len(parts) == 1 {
		return root
	}
	return bpFindInChildren(parts[1], root)
}

func bpFindInChildren(path string, node *blueprintNode) *blueprintNode {
	parts := strings.SplitN(path, "/", 2)
	for _, c := range node.children {
		if c.name == parts[0] {
			if len(parts) == 1 {
				return c
			}
			return bpFindInChildren(parts[1], c)
		}
	}
	return nil
}

func bpDeleteNode(id widget.TreeNodeID, root *blueprintNode) {
	parts := strings.Split(id, "/")
	if len(parts) < 2 {
		return
	}
	parentID := strings.Join(parts[:len(parts)-1], "/")
	targetName := parts[len(parts)-1]
	parent := bpFindNode(parentID, root)
	if parent == nil {
		return
	}
	for i, c := range parent.children {
		if c.name == targetName {
			parent.children = append(parent.children[:i], parent.children[i+1:]...)
			return
		}
	}
}

func bpDepth(id widget.TreeNodeID) int {
	return strings.Count(id, "/")
}

func bpRankForDepth(depth int) string {
	ranks := []string{"war", "theater", "battle", "strategy", "tactic", "skirmish"}
	if depth < 0 {
		depth = 0
	}
	if depth >= len(ranks) {
		depth = len(ranks) - 1
	}
	return ranks[depth]
}

func bpSerialize(node *blueprintNode, depth int) string {
	indent := strings.Repeat("  ", depth)
	var sb strings.Builder

	if node.isFile {
		sb.WriteString(fmt.Sprintf("%s%s", indent, node.name))
		if len(node.bullets) > 0 {
			sb.WriteString(" : ")
			sb.WriteString(strings.Join(node.bullets, " "))
		}
		sb.WriteString(";\n")
	} else {
		if depth == 0 {
			sb.WriteString(fmt.Sprintf("%s#rank: %s;\n", indent, node.rank))
		} else {
			sb.WriteString(fmt.Sprintf("%s%s/ {\n", indent, node.name))
		}
		if node.lang != "" {
			sb.WriteString(fmt.Sprintf("%s  #lang: %s;\n", indent, node.lang))
		}
		for _, c := range node.children {
			sb.WriteString(bpSerialize(c, depth+1))
		}
		if depth > 0 {
			sb.WriteString(fmt.Sprintf("%s}\n", indent))
		}
	}
	return sb.String()
}

func bpParse(content string) *blueprintNode {
	// Simple parse: rebuild tree from blueprint.bu lines
	root := &blueprintNode{name: "project", rank: "war"}
	stack := []*blueprintNode{root}

	for _, line := range strings.Split(content, "\n") {
		line = strings.TrimSpace(line)
		if line == "" || strings.HasPrefix(line, "//") {
			continue
		}
		if strings.HasPrefix(line, "#rank:") {
			rank := strings.TrimSuffix(strings.TrimPrefix(line, "#rank:"), ";")
			stack[len(stack)-1].rank = strings.TrimSpace(rank)
			continue
		}
		if strings.HasPrefix(line, "#lang:") {
			lang := strings.TrimSuffix(strings.TrimPrefix(line, "#lang:"), ";")
			stack[len(stack)-1].lang = strings.TrimSpace(lang)
			continue
		}
		if strings.HasSuffix(line, "{") {
			name := strings.TrimSuffix(strings.TrimSuffix(line, "{"), "/")
			name = strings.TrimSpace(filepath.Base(name))
			depth := len(stack)
			node := &blueprintNode{name: name, rank: bpRankForDepth(depth)}
			stack[len(stack)-1].children = append(stack[len(stack)-1].children, node)
			stack = append(stack, node)
			continue
		}
		if line == "}" {
			if len(stack) > 1 {
				stack = stack[:len(stack)-1]
			}
			continue
		}
		if strings.HasSuffix(line, ";") {
			line = strings.TrimSuffix(line, ";")
			parts := strings.SplitN(line, ":", 2)
			name := strings.TrimSpace(parts[0])
			var bullets []string
			if len(parts) > 1 {
				for _, b := range strings.Split(parts[1], " ") {
					if t := strings.TrimSpace(b); t != "" {
						bullets = append(bullets, t)
					}
				}
			}
			node := &blueprintNode{name: name, isFile: true, bullets: bullets}
			stack[len(stack)-1].children = append(stack[len(stack)-1].children, node)
		}
	}
	return root
}
