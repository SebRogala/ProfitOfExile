<?php

namespace App\Infrastructure\Http;

class PoeNinjaHttpClient extends HttpClient
{
    protected string $baseUrl = 'https://poe.ninja/api/data';

    private string $league = 'Sanctum';

    public function searchFor(string $key, array $data): mixed
    {
        foreach ($data['lines'] as $line) {
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
}
