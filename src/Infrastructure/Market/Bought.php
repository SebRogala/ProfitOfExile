<?php

namespace App\Infrastructure\Market;

use App\Domain\Inventory\Inventory;
use App\Domain\Item\Item;

class Bought
{
    public function __construct(private Item $item, private int $quantity)
    {
    }

    public function addToInventory(Inventory $inventory): void
    {
        $inventory->add($this->item, $this->quantity);
    }

    public function item(): Item
    {
        return $this->item;
    }

    public function quantity(): int
    {
        return $this->quantity;
    }
}
