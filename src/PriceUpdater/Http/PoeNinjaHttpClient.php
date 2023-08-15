<?php

namespace App\PriceUpdater\Http;

class PoeNinjaHttpClient extends HttpClient
{
    protected string $baseUrl = 'https://poe.ninja/api/data';

    private array $data = [];

    public function searchFor(string $key): mixed
    {
        if (empty($this->data)) {
            $this->data = array_merge_recursive(
                $this->data,
                $this->getCurrencyPrices(),
                $this->getFragmentPrices(),
                $this->getInvitationPrices(),
                $this->getMapPrices(),
            );
        }

        foreach ($this->data['lines'] as $line) {
            if ($line['detailsId'] == $key) {
                return $line;
            }
        }

        return null;
    }

    public function getCurrencyPrices(): array
    {
        return $this->get(sprintf('currencyoverview?league=%s&type=Currency', $this->league))->toArray();
    }

    public function getFragmentPrices(): array
    {
        return $this->get(sprintf('currencyoverview?league=%s&type=Fragment', $this->league))->toArray();
    }

    public function getInvitationPrices(): array
    {
        return $this->get(sprintf('itemoverview?league=%s&type=Invitation', $this->league))->toArray();
    }

    public function getMapPrices(): array
    {
        return $this->get(sprintf('itemoverview?league=%s&type=Map', $this->league))->toArray();
    }
}
