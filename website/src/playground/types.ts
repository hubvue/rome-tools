import type { Diagnostic } from "@rometools/wasm-web";
import type { Dispatch, SetStateAction } from "react";
import type { parser } from "codemirror-lang-rome-ast";

export enum IndentStyle {
	Tab = "tab",
	Space = "space",
}

export enum SourceType {
	Module = "module",
	Script = "script",
}

export enum QuoteStyle {
	Double = "double",
	Single = "single",
}

export enum QuoteProperties {
	AsNeeded = "as-needed",
	Preserve = "preserve",
}

export enum TrailingComma {
	All = "all",
	ES5 = "es5",
	None = "none",
}

export enum LoadingState {
	Loading,
	Success,
	Error,
}

export type PrettierOutput =
	| { type: "SUCCESS"; code: string; ir: string }
	| { type: "ERROR"; stack: string };

export const emptyPrettierOutput: PrettierOutput = {
	type: "SUCCESS",
	code: "",
	ir: "",
};

export interface RomeOutput {
	syntax: {
		ast: string;
		cst: string;
	};
	diagnostics: {
		console: string;
		list: Diagnostic[];
	};
	formatter: {
		code: string;
		ir: string;
	};
	analysis: {
		controlFlowGraph: string;
	};
}

export const emptyRomeOutput: RomeOutput = {
	syntax: {
		ast: "",
		cst: "",
	},
	diagnostics: {
		console: "",
		list: [],
	},
	formatter: {
		code: "",
		ir: "",
	},
	analysis: {
		controlFlowGraph: "",
	},
};

export interface PlaygroundSettings {
	lineWidth: number;
	indentStyle: IndentStyle;
	indentWidth: number;
	quoteStyle: QuoteStyle;
	quoteProperties: QuoteProperties;
	trailingComma: TrailingComma;
	enabledNurseryRules: boolean;
	enabledLinting: boolean;
}

export interface PlaygroundFileState {
	content: string;
	prettier: PrettierOutput;
	rome: RomeOutput;
}

export interface PlaygroundState {
	currentFile: string;
	singleFileMode: boolean;
	tab: string;
	cursorPosition: number;
	files: Record<string, undefined | PlaygroundFileState>;
	settings: PlaygroundSettings;
}

export const defaultPlaygroundState: PlaygroundState = {
	cursorPosition: 0,
	tab: "formatter",
	currentFile: "main.tsx",
	singleFileMode: false,
	files: {
		"main.tsx": {
			content: "",
			prettier: emptyPrettierOutput,
			rome: emptyRomeOutput,
		},
	},
	settings: {
		lineWidth: 80,
		indentWidth: 2,
		indentStyle: IndentStyle.Tab,
		quoteStyle: QuoteStyle.Double,
		quoteProperties: QuoteProperties.AsNeeded,
		trailingComma: TrailingComma.All,
		enabledNurseryRules: true,
		enabledLinting: true,
	},
};

export interface PlaygroundProps {
	setPlaygroundState: Dispatch<SetStateAction<PlaygroundState>>;
	resetPlaygroundState: () => void;
	playgroundState: PlaygroundState;
}

export type Tree = ReturnType<typeof parser.parse>;
type RangeMapKey = [number, number];
type RangeMapValue = [number, number];
export interface RomeAstSyntacticData {
	ast: Tree;
	// key is range of original `SyntaxToken`, value is the range string, like `20..20` corresponding range in
	// `rome_xx_ast` `Display` string.
	rangeMap: Map<RangeMapKey, RangeMapValue>;
}
