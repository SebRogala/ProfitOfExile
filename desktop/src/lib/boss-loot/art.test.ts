import { describe, expect, it, vi } from 'vitest'
import { bossLootIcon } from './art'
vi.mock('$lib/icons', () => ({ getIconUrl: (type: string, id: string) => `https://test/api/icon/${type}/${encodeURIComponent(id)}` }))
describe('boss loot typed art', () => {
	it('uses the items cache for exact uniques', () => {
		expect(bossLootIcon('The Devouring Diadem')).toBe('https://test/api/icon/items/The%20Devouring%20Diadem')
	})
	it('uses the items cache for Communion', () => {
		expect(bossLootIcon('Communion Support')).toBe('https://test/api/icon/items/Communion%20Support')
	})
	it('matches exchange IconPath escaping for catalogue ids', () => {
		expect(bossLootIcon('Metadata/Items/DivinationCards/DivinationCardNooksCrown')).toBe('https://test/api/icon/currency-exchange/Metadata%2FItems%2FDivinationCards%2FDivinationCardNooksCrown')
	})
	it('leaves the explicit any placeholder for categories', () => {
		expect(bossLootIcon('category:catarina:1:rare-two-veiled-modifiers')).toBeNull()
	})
})
