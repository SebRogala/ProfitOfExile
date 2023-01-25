<?php

namespace App\Application\Command\PriceRegistry;

use App\Domain\Item\Currency\BlueLifeforce;
use App\Domain\Item\Currency\ChaosOrb;
use App\Domain\Item\Currency\DivineOrb;
use App\Domain\Item\Currency\OrbOfScouring;
use App\Domain\Item\Currency\PurpleLifeforce;
use App\Domain\Item\Currency\YellowLifeforce;
use App\Domain\Item\Fragment\MavenSplinter;
use App\Domain\Item\Fragment\ShaperGuardianFragment;
use App\Domain\Item\Map\MavenWrit;
use App\Domain\Item\Map\ShaperGuardianMap;
use App\Domain\Item\Map\TheFormed;
use App\Domain\Item\Set\ShaperSet;
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
                'item' => ChaosOrb::class,
                'ninjaInChaos' => 1,
            ],
            [
                'item' => DivineOrb::class,
                'ninjaInChaos' => $divPrice,
            ],
            [
                'item' => OrbOfScouring::class,
                'ninjaInChaos' => $this->poeNinjaHttpClient->searchFor('orb-of-scouring')['chaosEquivalent'],
            ],
            [
                'item' => YellowLifeforce::class,
                'ninjaInChaos' => $this->poeNinjaHttpClient->searchFor('vivid-crystallised-lifeforce')['receive']['value'],
            ],
            [
                'item' => BlueLifeforce::class,
                'ninjaInChaos' => $this->poeNinjaHttpClient->searchFor('primal-crystallised-lifeforce')['receive']['value'],
            ],
            [
                'item' => PurpleLifeforce::class,
                'ninjaInChaos' => $this->poeNinjaHttpClient->searchFor('wild-crystallised-lifeforce')['receive']['value'],
            ],
            [
                'item' => ShaperGuardianMap::class,
                'tftInChaos' => $this->tftHttpClient->searchFor('Shaper Maps')['chaos'],
            ],
            [
                'item' => ShaperGuardianFragment::class,
//                'tftInChaos' => $this->tftHttpClient->searchFor('Shaper Set')['chaos'],
                'ninjaInChaos' => $this->calculatePriceOfFour(
                    $this->poeNinjaHttpClient->searchFor('fragment-of-the-hydra')['pay']['value'],
                    $this->poeNinjaHttpClient->searchFor('fragment-of-the-minotaur')['pay']['value'],
                    $this->poeNinjaHttpClient->searchFor('fragment-of-the-chimera')['pay']['value'],
                    $this->poeNinjaHttpClient->searchFor('fragment-of-the-phoenix')['pay']['value'],
                )
            ],
            [
                'item' => ShaperSet::class,
//                'tftInChaos' => $this->tftHttpClient->searchFor('Shaper Set')['chaos'],
                'tftInChaos' => $divPrice / 3,
                'ninjaInChaos' => $this->calculateSumPriceOfFour(
                    $this->poeNinjaHttpClient->searchFor('fragment-of-the-hydra')['pay']['value'],
                    $this->poeNinjaHttpClient->searchFor('fragment-of-the-minotaur')['pay']['value'],
                    $this->poeNinjaHttpClient->searchFor('fragment-of-the-chimera')['pay']['value'],
                    $this->poeNinjaHttpClient->searchFor('fragment-of-the-phoenix')['pay']['value'],
                )
            ],
            [
                'item' => TheFormed::class,
                'tftInChaos' => $this->tftHttpClient->searchFor('The Formed')['chaos'],
                'ninjaInChaos' => $this->poeNinjaHttpClient->searchFor('mavens-invitation:-the-formed')['chaosValue'],
            ],
            [
                'item' => MavenSplinter::class,
                'ninjaInChaos' => 1/$this->poeNinjaHttpClient->searchFor('crescent-splinter')['chaosEquivalent'],
            ],
            [
                'item' => MavenWrit::class,
                'tftInChaos' => $this->tftHttpClient->searchFor('Maven\'s Writ')['chaos'],
                'ninjaInChaos' => $this->poeNinjaHttpClient->searchFor('the-mavens-writ')['receive']['value'],
            ],
        ];

        $jsonString = json_encode($jsonData, JSON_PRETTY_PRINT);
        $fp = fopen($this->path, 'w');
        fwrite($fp, $jsonString);
        fclose($fp);
    }

    private function calculatePriceOfFour($price1, $price2, $price3, $price4): float
    {
        return $this->calculateSumPriceOfFour($price1, $price2, $price3, $price4) / 4;
    }

    private function calculateSumPriceOfFour($price1, $price2, $price3, $price4): float
    {
        return (1/$price1 + 1/$price2 + 1/$price3 + 1/$price4);
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
