<?php

namespace App\Infrastructure\Pricer;

use App\Application\Query\Pricer\PricesQuery;
use App\Domain\Inventory\Inventory;

class Pricer
{
    public function __construct(private PricesQuery $pricesQuery)
    {
    }

    public function priceInventory(Inventory $inventory, array $boughtSummary = []): array
    {
        $result = [
            'totalWorthInChaos' => 0,
            'items' => [],
        ];

        $items = $inventory->getItems();

        foreach ($items as $item => $quantity) {
            $itemPriceData = $this->pricesQuery->findDataFor($item);

            if (isset($itemPriceData['ninjaInChaos'])) {
                $price = $itemPriceData['ninjaInChaos'];
            } else {
                $price = $itemPriceData['tftInChaos'];
            }

            $result['totalWorthInChaos'] += $price * $quantity;
            $result['items'][$item] = [
                'singularPrice' => $price,
                'quantity' => $quantity,
                'summedPrice' => $price * $quantity,
            ];
        }

        if (!empty($boughtSummary)) {
            $result = $this->calculateSummary($result, $boughtSummary);
        }

        return $result;
    }

    private function calculateSummary($result, $boughtSummary)
    {
        $result['bought'] = $boughtSummary;

        foreach ($boughtSummary as $item) {
            $result['profit'] = $result['totalWorthInChaos'] - $item['totalPrice'];
        }

        return $result;
    }
}
