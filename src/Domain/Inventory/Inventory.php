<?php

namespace App\Domain\Inventory;

use App\Domain\Item\Item;
use App\Infrastructure\Market\Buyer;

class Inventory
{
    public function __construct(private SetConverter $setConverter, private Buyer $buyer)
    {
    }

    public function add(Item $item, int $quantity = 1): void
    {
        $this->items[$item::class] = @(int)$this->items[$item::class] + $quantity;
        $this->setConverter->convertToSets($this);
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
            $boughtItems = $this->buyer->buy($item, $quantity);
            $this->add($boughtItems->item(), $boughtItems->quantity());
        }

        $this->items[$item::class] = $this->items[$item::class] - $quantity;

        if ($this->items[$item::class] === 0) {
            unset($this->items[$item::class]);
        }
    }
}
