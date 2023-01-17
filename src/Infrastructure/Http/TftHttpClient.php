<?php

namespace App\Infrastructure\Http;

class TftHttpClient extends HttpClient
{
    protected string $baseUrl = 'https://raw.githubusercontent.com/The-Forbidden-Trove/tft-data-prices/master/lsc';

    private array $data = [];

    public function searchFor(string $key): mixed
    {
        if (empty($this->data)) {
            $this->data = array_merge_recursive(
                $this->data,
                $this->getBulkSetsPrices(),
                $this->getBulkInvitationPrices(),
                $this->getBulkMapsPrices(),
            );
        }

        foreach ($this->data['data'] as $line) {
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

    public function getBulkMapsPrices(): array
    {
        return $this->get('bulk-maps.json')->toArray();
    }
}
