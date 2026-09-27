const MARKDOWN_EXTENSION = /\.(md|markdown|mdx|mdown)$/i

export function isMarkdownPath(path: string): boolean {
  const name = path.split(/[\\/]/).pop() ?? path
  return MARKDOWN_EXTENSION.test(name)
}
