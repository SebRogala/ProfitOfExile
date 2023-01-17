<?php

namespace App\Application\Command\PriceRegistry;

use App\Domain\Item\Currency\DivineOrb;
use App\Domain\Item\Fragment\ShaperGuardianFragment;
use App\Domain\Item\Map\ShaperGuardianMap;
use App\Domain\Strategy\ShaperFragmentsStrategy;
use App\Infrastructure\Http\PoeNinjaHttpClient;
use App\Infrastructure\Http\TftHttpClient;

class UpdateRegistryHandler
{
    private string $path;

    public function __construct(
        private string $dataDir,
        private string $priceRegistryFile,
        private PoeNinjaHttpClient $poeNinjaHttpClient,
        private TftHttpClient $tftHttpClient
    ) {
        $this->path = $this->dataDir.'/'.$this->priceRegistryFile;
    }

    public function handle(UpdateRegistry $command): void
    {
        if (!$this->shouldUpdate($command->shouldForceUpdate())) {
            return;
        }

        $divPrice = $this->poeNinjaHttpClient->searchFor('divine-orb')['chaosEquivalent'];

        $jsonData = [
            'timestamp' => (new \DateTime())->format('U'),
            [
                'item' => DivineOrb::class,
                'ninjaInChaos' => $divPrice,
            ],
            [
                'item' => ShaperGuardianMap::class,
                'tftInChaos' => $this->tftHttpClient->searchFor('Shaper Maps')['chaos'],
            ],
            [
                'item' => ShaperGuardianFragment::class,
                'tftInChaos' => $this->tftHttpClient->searchFor('Shaper Set')['chaos'],
                'ninjaInChaos' => $this->calculatePriceOfFour(
                    $this->poeNinjaHttpClient->searchFor('fragment-of-the-hydra')['pay']['value'],
                    $this->poeNinjaHttpClient->searchFor('fragment-of-the-minotaur')['pay']['value'],
                    $this->poeNinjaHttpClient->searchFor('fragment-of-the-chimera')['pay']['value'],
                    $this->poeNinjaHttpClient->searchFor('fragment-of-the-phoenix')['pay']['value'],
                )
            ],
        ];

        $jsonString = json_encode($jsonData, JSON_PRETTY_PRINT);
        $fp = fopen($this->path, 'w');
        fwrite($fp, $jsonString);
        fclose($fp);
    }

    private function calculatePriceOfFour($price1, $price2, $price3, $price4): float
    {
        return (1/$price1 + 1/$price2 + 1/$price3 + 1/$price4) / 4;
    }

    private function shouldUpdate(bool $shouldForceUpdate): bool
    {
        if ($shouldForceUpdate | !file_exists($this->path)) {
            return true;
        }

        $jsonString = file_get_contents($this->path);
        $jsonData = json_decode($jsonString, true);

        $diff = (new \DateTime())->diff((new \DateTime())->setTimestamp($jsonData['timestamp']));

        return $diff->i > 60;
    }
}
