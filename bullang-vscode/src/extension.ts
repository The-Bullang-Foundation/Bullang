import * as path from 'path';
import { workspace, ExtensionContext, window } from 'vscode';
import {
    LanguageClient,
    LanguageClientOptions,
    ServerOptions,
    TransportKind,
} from 'vscode-languageclient/node';

let client: LanguageClient | undefined;

export function activate(context: ExtensionContext) {
    const config     = workspace.getConfiguration('bullang');
    const serverPath = config.get<string>('serverPath', 'bullang');

    const serverOptions: ServerOptions = {
      run:   { command: serverPath, args: ['lsp'] },
      debug: { command: serverPath, args: ['lsp'] },
    };

    const clientOptions: LanguageClientOptions = {
        documentSelector: [{ scheme: 'file', language: 'bullang' }],
        synchronize: {
            fileEvents: workspace.createFileSystemWatcher('**/*.bu'),
        },
    };

    client = new LanguageClient(
        'bullang',
        'Bullang Language Server',
        serverOptions,
        clientOptions,
    );

    client.start().catch(err => {
        window.showErrorMessage(
            `Bullang language server failed to start: ${err.message}\n` +
            `Make sure 'bullang' is on your PATH, or set bullang.serverPath in settings.`
        );
    });
}

export function deactivate(): Thenable<void> | undefined {
    return client?.stop();
}
