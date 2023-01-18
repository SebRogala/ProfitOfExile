<?php

namespace App\Domain\Inventory;

use App\Domain\Item\Item;

class Inventory
{
    private array $items = [];

    public function add(Item $item, int $quantity = 1): void
    {
        $this->items[$item::class] = @(int)$this->items[$item::class] + $quantity;
    }

    public function getItems(): array
    {
        return $this->items;
    }

    public function hasItems(Item $item, $quantity = 1): bool
    {
        if (empty($this->items[$item::class])) {
            return false;
        }

        if ($this->items[$item::class] < $quantity) {
            return false;
        }

        return true;
    }

    public function removeItems(Item $item, int $quantity = 1): void
    {
        if (!$this->hasItems($item, $quantity)) {
            throw new ItemNotFoundInInventoryException();
        }

        $this->items[$item::class] = $this->items[$item::class] - $quantity;

        if ($this->items[$item::class] === 0) {
            unset($this->items[$item::class]);
        }
    }
}
