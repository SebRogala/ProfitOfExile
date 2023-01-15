<?php

namespace App\Domain\Trait;

use Doctrine\ORM\Mapping as ORM;

trait Timestamps
{
    #[ORM\Column(type: 'datetime_immutable')]
    private \DateTimeImmutable $createdAt;

    #[ORM\Column(type: 'datetime_immutable')]
    private \DateTimeImmutable $updatedAt;

    private function timestamps(): void
    {
        $this->createdAt = new \DateTimeImmutable();
        $this->setUpdatedAt();
    }

    public function setUpdatedAt(): void
    {
        $this->updatedAt = new \DateTimeImmutable();
    }
}
