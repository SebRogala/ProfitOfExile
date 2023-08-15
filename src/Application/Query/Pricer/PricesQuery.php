<?php

namespace App\Application\Query\Pricer;

use App\Item\Item;

interface PricesQuery
{
    public function findDataFor(Item $item): array;
    public function getDivinePrice(): float;
}
