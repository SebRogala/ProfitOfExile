<?php

namespace App\Application\Command\PriceRegistry;

class UpdateRegistry
{
    public function __construct(private bool $shouldForceUpdate = false)
    {
    }

    public function shouldForceUpdate(): bool
    {
        return $this->shouldForceUpdate;
    }
}
