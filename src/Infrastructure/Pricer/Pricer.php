<?php

namespace App\Infrastructure\Pricer;

use App\Application\Query\Pricer\PricesQuery;
use App\Domain\Inventory\Inventory;

class Pricer
{
    public function __construct(private PricesQuery $pricesQuery)
    {
    }

    public function priceInventory(Inventory $inventory): array
    {
        $result = [
            'totalChaos' => 0,
        ];

        $items = $inventory->getItems();

        foreach ($items as $item => $quantity) {
            $itemPriceData = $this->pricesQuery->findDataFor($item);

            if (isset($itemPriceData['ninjaInChaos'])) {
                $result['totalChaos'] += $itemPriceData['ninjaInChaos'] * $quantity;
            } else {
                $result['totalChaos'] += $itemPriceData['tftInChaos'] * $quantity;
            }
        }

        return $result;
    }
}
