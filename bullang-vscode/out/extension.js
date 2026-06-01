"use strict";
Object.defineProperty(exports, "__esModule", { value: true });
exports.activate = activate;
exports.deactivate = deactivate;
const vscode_1 = require("vscode");
const node_1 = require("vscode-languageclient/node");
let client;
function activate(context) {
    const config = vscode_1.workspace.getConfiguration('bullang');
    const serverPath = config.get('serverPath', 'bullang');
    const serverOptions = {
        command: serverPath,
        args: ['lsp'],
    };
    const clientOptions = {
        documentSelector: [{ scheme: 'file', language: 'bullang' }],
        synchronize: {
            fileEvents: vscode_1.workspace.createFileSystemWatcher('**/*.bu'),
        },
    };
    client = new node_1.LanguageClient('bullang', 'Bullang Language Server', serverOptions, clientOptions);
    client.start().catch(err => {
        vscode_1.window.showErrorMessage(`Bullang language server failed to start: ${err.message}\n` +
            `Make sure 'bullang' is on your PATH, or set bullang.serverPath in settings.`);
    });
}
function deactivate() {
    return client?.stop();
}
//# sourceMappingURL=extension.js.map