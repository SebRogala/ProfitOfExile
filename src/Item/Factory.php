<?php

declare(strict_types=1);

namespace App\Item;

use App\Item\Exception\ItemNotFoundException;
use App\Item\ItemPrice\ItemPriceRepository;

class Factory
{
    public function __construct(private ItemPriceRepository $itemPriceRepository)
    {
    }

    public function create(string $nameKey): Item
    {
        $itemPrice = $this->itemPriceRepository->getByNameKey($nameKey);
        if (!$itemPrice) {
            throw new ItemNotFoundException();
        }

        return new $itemPrice->namespace();
    }
}
