<?php

namespace App\Infrastructure\Http;

use Symfony\Contracts\HttpClient\HttpClientInterface;
use Symfony\Contracts\HttpClient\ResponseInterface;

abstract class HttpClient
{
    protected string $baseUrl = '';

    public function __construct(protected HttpClientInterface $client)
    {
    }

    abstract public function searchFor(string $key, array $data): mixed;

    protected function get(string $path = ''): ResponseInterface
    {
        return $this->client->request(
            'GET',
            $this->baseUrl . '/' . $path
        );
    }
}
