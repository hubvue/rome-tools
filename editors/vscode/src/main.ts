import { spawn } from "child_process";
import { connect } from "net";
import { promisify } from "util";
import {
	ExtensionContext,
	languages,
	TextEditor,
	Uri,
	window,
	workspace,
} from "vscode";
import {
	DocumentFilter,
	LanguageClient,
	LanguageClientOptions,
	ServerOptions,
	StreamInfo,
} from "vscode-languageclient/node";
import { isAbsolute } from "path";
import { setContextValue } from "./utils";
import { Session } from "./session";
import { syntaxTree } from "./commands/syntaxTree";
import { Commands } from "./commands";
import { StatusBar } from "./statusBar";

import resolveImpl = require("resolve/async");
import type * as Resolve from "resolve";

const resolveAsync = promisify<string, Resolve.AsyncOpts, string | undefined>(
	resolveImpl,
);

let client: LanguageClient;

const IN_ROME_PROJECT = "inRomeProject";

export async function activate(context: ExtensionContext) {
	const command = await getServerPath(context);

	if (!command) {
		await window.showErrorMessage(
			"The Rome extensions doesn't ship with prebuilt binaries for your platform yet. " +
				"You can still use it by cloning the rome/tools repo from GitHub to build the LSP " +
				"yourself and use it with this extension with the rome.lspBin setting",
		);
		return;
	}

	const statusBar = new StatusBar();

	const serverOptions: ServerOptions = createMessageTransports.bind(
		undefined,
		command,
	);

	const traceOutputChannel = window.createOutputChannel("Rome Trace");

	const documentSelector: DocumentFilter[] = [
		{ language: "javascript" },
		{ language: "typescript" },
		{ language: "javascriptreact" },
		{ language: "typescriptreact" },
	];

	const clientOptions: LanguageClientOptions = {
		documentSelector,
		traceOutputChannel,
	};

	client = new LanguageClient("rome_lsp", "Rome", serverOptions, clientOptions);

	const session = new Session(context, client);

	const codeDocumentSelector =
		client.protocol2CodeConverter.asDocumentSelector(documentSelector);

	// we are now in a rome project
	setContextValue(IN_ROME_PROJECT, true);

	session.registerCommand(Commands.SyntaxTree, syntaxTree(session));
	session.registerCommand(Commands.ServerStatus, () => {
		traceOutputChannel.show();
	});
	session.registerCommand(Commands.RestartLspServer, () => {
		client.restart().catch((error) => {
			client.error("Restarting client failed", error, "force");
		});
	});

	context.subscriptions.push(
		client.onDidChangeState((evt) => {
			statusBar.setServerState(client, evt.newState);
		}),
	);

	const handleActiveTextEditorChanged = (textEditor?: TextEditor) => {
		if (!textEditor) {
			statusBar.setActive(false);
			return;
		}

		const { document } = textEditor;
		statusBar.setActive(languages.match(codeDocumentSelector, document) > 0);
	};

	context.subscriptions.push(
		window.onDidChangeActiveTextEditor(handleActiveTextEditorChanged),
	);

	handleActiveTextEditorChanged(window.activeTextEditor);
	client.start();
}

type Architecture = "x64" | "arm64";

type PlatformTriplets = {
	[P in NodeJS.Platform]?: {
		[A in Architecture]: {
			triplet: string;
			package: string;
		};
	};
};

const PLATFORMS: PlatformTriplets = {
	win32: {
		x64: {
			triplet: "x86_64-pc-windows-msvc",
			package: "@rometools/cli-win32-x64/rome.exe",
		},
		arm64: {
			triplet: "aarch64-pc-windows-msvc",
			package: "@rometools/cli-win32-arm64/rome.exe",
		},
	},
	darwin: {
		x64: {
			triplet: "x86_64-apple-darwin",
			package: "@rometools/cli-darwin-x64/rome",
		},
		arm64: {
			triplet: "aarch64-apple-darwin",
			package: "@rometools/cli-darwin-arm64/rome",
		},
	},
	linux: {
		x64: {
			triplet: "x86_64-unknown-linux-gnu",
			package: "@rometools/cli-linux-x64/rome",
		},
		arm64: {
			triplet: "aarch64-unknown-linux-gnu",
			package: "@rometools/cli-linux-arm64/rome",
		},
	},
};

async function getServerPath(
	context: ExtensionContext,
): Promise<string | undefined> {
	// Only allow the bundled Rome binary in untrusted workspaces
	if (!workspace.isTrusted) {
		return getBundledBinary(context);
	}

	if (process.env.DEBUG_SERVER_PATH) {
		window.showInformationMessage(
			`Rome DEBUG_SERVER_PATH detected: ${process.env.DEBUG_SERVER_PATH}`,
		);
		return process.env.DEBUG_SERVER_PATH;
	}

	const config = workspace.getConfiguration();
	const explicitPath = config.get("rome.lspBin");
	if (typeof explicitPath === "string" && explicitPath !== "") {
		return getWorkspaceRelativePath(explicitPath);
	}

	return (await getWorkspaceDependency()) ?? getBundledBinary(context);
}

// Resolve `path` as relative to the workspace root
async function getWorkspaceRelativePath(path: string) {
	if (isAbsolute(path)) {
		return path;
	} else {
		for (let i = 0; i < workspace.workspaceFolders.length; i++) {
			const workspaceFolder = workspace.workspaceFolders[i];
			const possiblePath = Uri.joinPath(workspaceFolder.uri, path);
			if (await fileExists(possiblePath)) {
				return possiblePath.fsPath;
			}
		}
		return undefined;
	}
}

// Tries to resolve a path to `@rometools/cli-*` binary package from the root of the workspace
async function getWorkspaceDependency(): Promise<string | undefined> {
	const packageName = PLATFORMS[process.platform]?.[process.arch]?.package;

	for (const workspaceFolder of workspace.workspaceFolders) {
		try {
			const result = await resolveAsync(packageName, {
				basedir: workspaceFolder.uri.fsPath,
			});

			if (result) {
				return result;
			}
		} catch {}
	}

	return undefined;
}

// Returns the path of the binary distribution of Rome included in the bundle of the extension
async function getBundledBinary(context: ExtensionContext) {
	const triplet = PLATFORMS[process.platform]?.[process.arch]?.triplet;
	if (!triplet) {
		return undefined;
	}

	const binaryExt = triplet.includes("windows") ? ".exe" : "";
	const binaryName = `rome${binaryExt}`;

	const bundlePath = Uri.joinPath(context.extensionUri, "server", binaryName);
	const bundleExists = await fileExists(bundlePath);

	return bundleExists ? bundlePath.fsPath : undefined;
}

async function fileExists(path: Uri) {
	try {
		await workspace.fs.stat(path);
		return true;
	} catch (err) {
		if (err.code === "ENOENT") {
			return false;
		} else {
			throw err;
		}
	}
}

function getSocket(command: string): Promise<string> {
	return new Promise((resolve, reject) => {
		const process = spawn(command, ["__print_socket"], {
			stdio: "pipe",
		});

		process.on("error", reject);

		let pipeName = "";
		process.stdout.on("data", (data) => {
			pipeName += data.toString("utf-8");
		});

		process.on("exit", (code) => {
			if (code === 0) {
				console.log(`"${pipeName}"`);
				resolve(pipeName.trimEnd());
			} else {
				reject(code);
			}
		});
	});
}

async function createMessageTransports(command: string): Promise<StreamInfo> {
	const path = await getSocket(command);
	const socket = connect(path);

	await new Promise((resolve, reject) => {
		socket.once("error", reject);
		socket.once("ready", resolve);
	});

	return { writer: socket, reader: socket };
}

export function deactivate(): Thenable<void> | undefined {
	if (!client) {
		return undefined;
	}
	return client.stop();
}
