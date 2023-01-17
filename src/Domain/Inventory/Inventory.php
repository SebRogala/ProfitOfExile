<?php

namespace App\Domain\Inventory;

use App\Domain\Item\Item;

class Inventory
{
    private array $items = [];

    public function add(Item $item, int $quantity = 1):void
    {
        $this->items[] = $item;
    }
}
