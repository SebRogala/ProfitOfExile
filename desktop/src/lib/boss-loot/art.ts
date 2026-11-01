import { getIconUrl } from '$lib/icons'

/** Catalogue ids match exchange.IconPath; unique names use the typed items cache. */
export function bossLootIcon(id: string): string | null {
	if (id.startsWith('category:')) return null
	return getIconUrl(id.startsWith('Metadata/') ? 'currency-exchange' : 'items', id)
}
