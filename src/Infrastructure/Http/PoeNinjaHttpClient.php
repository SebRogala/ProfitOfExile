<?php

namespace App\Infrastructure\Http;

class PoeNinjaHttpClient extends HttpClient
{
    protected string $baseUrl = 'https://poe.ninja/api/data';

    private string $league = 'Sanctum';

    private array $data = [];

    public function searchFor(string $key): mixed
    {
        if (empty($this->data)) {
            array_merge(
                $this->data,
                $this->getCurrencyPrices(),
                $this->getFragmentPrices(),
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
        return $this->get(sprintf('currencyoverview?league=%s&type=Currency&language=en', $this->league))->toArray();
    }

    public function getFragmentPrices(): array
    {
        return $this->get(sprintf('currencyoverview?league=%s&type=Fragment&language=en', $this->league))->toArray();
    }
}
