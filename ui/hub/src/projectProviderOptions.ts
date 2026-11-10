function uniqueTrimmed(values: ReadonlyArray<string | null | undefined>): string[] {
  const seen = new Set<string>()
  const result: string[] = []
  for (const value of values) {
    const trimmed = value?.trim()
    if (!trimmed || seen.has(trimmed)) continue
    seen.add(trimmed)
    result.push(trimmed)
  }
  return result
}

export function projectProviderChoices(input: {
  availableProviders?: readonly string[] | null
  globalProviderNames?: readonly string[] | null
  current?: ReadonlyArray<string | null | undefined>
  overrides?: readonly string[]
}): string[] {
  const catalog =
    input.availableProviders != null
      ? input.availableProviders
      : input.globalProviderNames != null
        ? input.globalProviderNames
        : []
  const names = uniqueTrimmed([
    ...catalog,
    ...(input.current ?? []),
    ...(input.overrides ?? []),
  ])
  names.sort((left, right) => left.localeCompare(right))
  return names
}

export function projectModelChoices(input: {
  provider?: string | null
  providerModels?: Readonly<Record<string, readonly string[] | undefined>> | null
  globalModels?: readonly string[] | null
  current?: string | null
}): string[] {
  const provider = input.provider?.trim() ?? ''
  const catalog = input.providerModels
  const listed =
    provider && catalog && Object.prototype.hasOwnProperty.call(catalog, provider)
      ? (catalog[provider] ?? [])
      : (input.globalModels ?? [])
  return uniqueTrimmed([...listed, input.current])
}
