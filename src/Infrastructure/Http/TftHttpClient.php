<?php

namespace App\Infrastructure\Http;

class TftHttpClient extends HttpClient
{
    protected string $baseUrl = 'https://raw.githubusercontent.com/The-Forbidden-Trove/tft-data-prices/master/lsc';

    public function searchFor(string $key, array $data): mixed
    {
        foreach ($data['data'] as $line) {
            if ($line['name'] == $key) {
                return $line;
            }
        }

        return null;
    }

    public function getBulkInvitationPrices(): array
    {
        return $this->get('bulk-invitation.json')->toArray();
    }

    public function getBulkSetsPrices(): array
    {
        return $this->get('bulk-sets.json')->toArray();
    }
}
