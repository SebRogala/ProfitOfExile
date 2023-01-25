<?php

namespace App\Infrastructure\Pricer;

use App\Application\Query\Pricer\PricesQuery;
use App\Domain\Inventory\Inventory;
use App\Domain\Item\Fragment\ShaperGuardianFragment;
use App\Domain\Item\Set\ShaperSet;

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

        $items = $this->convertToSets($inventory);

        foreach ($items as $item => $quantity) {
            $itemPriceData = $this->pricesQuery->findDataFor($item);

            if (isset($itemPriceData['tftInChaos'])) {
                $price = $itemPriceData['tftInChaos'];
            } else {
                $price = $itemPriceData['ninjaInChaos'];
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

    private function convertToSets(Inventory $inventory): array
    {
        $shaperGuardianFragment = new ShaperGuardianFragment();
        while ($inventory->hasItems($shaperGuardianFragment, 4)) {
            $inventory->removeItems($shaperGuardianFragment, 4);
            $inventory->add(new ShaperSet());
        }

        return $inventory->getItems();
    }
}
