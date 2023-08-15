<?php

namespace App\Application\Command\PriceRegistry;

use App\Item\Currency\BlueLifeforce;
use App\Item\Currency\ChaosOrb;
use App\Item\Currency\DivineOrb;
use App\Item\Currency\OrbOfScouring;
use App\Item\Currency\PurpleLifeforce;
use App\Item\Currency\YellowLifeforce;
use App\Item\Fragment\ElderGuardianFragment;
use App\Item\Fragment\MavenSplinter;
use App\Item\Fragment\ShaperGuardianFragment;
use App\Item\Fragment\UberElderElderFragment;
use App\Item\Fragment\UberElderShaperFragment;
use App\Item\Map\ElderGuardianMap;
use App\Item\Map\MavenWrit;
use App\Item\Map\ShaperGuardianMap;
use App\Item\Map\TheFormed;
use App\Item\Map\TheTwisted;
use App\Item\Set\ElderSet;
use App\Item\Set\ShaperSet;
use App\Item\Set\UberElderSet;
use App\PriceUpdater\Http\PoeNinjaHttpClient;
use App\PriceUpdater\Http\TftHttpClient;

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
                'tftInChaos' => $this->tftHttpClient->searchFor('Vivid (Yellow)')['chaos'] / 1000,
            ],
            [
                'item' => BlueLifeforce::class,
                'ninjaInChaos' => $this->poeNinjaHttpClient->searchFor('primal-crystallised-lifeforce')['receive']['value'],
                'tftInChaos' => $this->tftHttpClient->searchFor('Primal (Blue)')['chaos'] / 1000,
            ],
            [
                'item' => PurpleLifeforce::class,
                'ninjaInChaos' => $this->poeNinjaHttpClient->searchFor('wild-crystallised-lifeforce')['receive']['value'],
                'tftInChaos' => $this->tftHttpClient->searchFor('Wild (Purple)')['chaos'] / 1000,
            ],
            [
                'item' => ShaperGuardianMap::class,
                'tftInChaos' => $this->tftHttpClient->searchFor('Shaper Maps')['chaos'],
            ],
            [
                'item' => ElderGuardianMap::class,
                'tftInChaos' => $this->tftHttpClient->searchFor('Elder Maps')['chaos'],
            ],
            [
                'item' => ShaperGuardianFragment::class,
//                'tftInChaos' => $this->tftHttpClient->searchFor('Shaper Set')['chaos'],
                'ninjaInChaos' => $this->calculatePriceOfFour(
                    $this->poeNinjaHttpClient->searchFor('fragment-of-the-hydra')['chaosEquivalent'],
                    $this->poeNinjaHttpClient->searchFor('fragment-of-the-minotaur')['chaosEquivalent'],
                    $this->poeNinjaHttpClient->searchFor('fragment-of-the-chimera')['chaosEquivalent'],
                    $this->poeNinjaHttpClient->searchFor('fragment-of-the-phoenix')['chaosEquivalent'],
                )
            ],
            [
                'item' => ElderGuardianFragment::class,
                'ninjaInChaos' => $this->calculatePriceOfFour(
                    $this->poeNinjaHttpClient->searchFor('fragment-of-purification')['chaosEquivalent'],
                    $this->poeNinjaHttpClient->searchFor('fragment-of-enslavement')['chaosEquivalent'],
                    $this->poeNinjaHttpClient->searchFor('fragment-of-constriction')['chaosEquivalent'],
                    $this->poeNinjaHttpClient->searchFor('fragment-of-eradication')['chaosEquivalent'],
                )
            ],
            [
                'item' => ShaperSet::class,
                'tftInChaos' => $this->tftHttpClient->searchFor('Shaper Set')['chaos'],
//                'tftInChaos' => $divPrice / 3,
                'ninjaInChaos' => $this->calculateSumPriceOfFour(
                    $this->poeNinjaHttpClient->searchFor('fragment-of-the-hydra')['chaosEquivalent'],
                    $this->poeNinjaHttpClient->searchFor('fragment-of-the-minotaur')['chaosEquivalent'],
                    $this->poeNinjaHttpClient->searchFor('fragment-of-the-chimera')['chaosEquivalent'],
                    $this->poeNinjaHttpClient->searchFor('fragment-of-the-phoenix')['chaosEquivalent'],
                )
            ],
            [
                'item' => ElderSet::class,
                'tftInChaos' => $this->tftHttpClient->searchFor('Elder Set')['chaos'],
                'ninjaInChaos' => $this->calculateSumPriceOfFour(
                    $this->poeNinjaHttpClient->searchFor('fragment-of-purification')['chaosEquivalent'],
                    $this->poeNinjaHttpClient->searchFor('fragment-of-enslavement')['chaosEquivalent'],
                    $this->poeNinjaHttpClient->searchFor('fragment-of-constriction')['chaosEquivalent'],
                    $this->poeNinjaHttpClient->searchFor('fragment-of-eradication')['chaosEquivalent'],
                )
            ],
            [
                'item' => UberElderShaperFragment::class,
                'ninjaInChaos' => ($this->poeNinjaHttpClient->searchFor('fragment-of-shape')['chaosEquivalent']
                + $this->poeNinjaHttpClient->searchFor('fragment-of-knowledge')['chaosEquivalent']) / 2,
            ],
            [
                'item' => UberElderElderFragment::class,
                'ninjaInChaos' => ($this->poeNinjaHttpClient->searchFor('fragment-of-emptiness')['chaosEquivalent']
                + $this->poeNinjaHttpClient->searchFor('fragment-of-terror')['chaosEquivalent']) / 2,
            ],
            [
                'item' => UberElderSet::class,
                'tftInChaos' => $this->tftHttpClient->searchFor('Uber Elder Set')['chaos'],
            ],
            [
                'item' => TheFormed::class,
                'tftInChaos' => $this->tftHttpClient->searchFor('The Formed')['chaos'],
                'ninjaInChaos' => $this->poeNinjaHttpClient->searchFor('mavens-invitation:-the-formed')['chaosValue'],
            ],
            [
                'item' => TheTwisted::class,
                'tftInChaos' => $this->tftHttpClient->searchFor('The Twisted')['chaos'],
                'ninjaInChaos' => $this->poeNinjaHttpClient->searchFor('mavens-invitation:-the-twisted')['chaosValue'],
            ],
            [
                'item' => MavenSplinter::class,
                'ninjaInChaos' => $this->poeNinjaHttpClient->searchFor('crescent-splinter')['chaosEquivalent'],
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
        return ($price1 + $price2 + $price3 + $price4);
    }

    private function shouldUpdate(bool $shouldForceUpdate): bool
    {
        if ($shouldForceUpdate | !file_exists($this->path)) {
            return true;
        }

        $jsonString = file_get_contents($this->path);
        $jsonData = json_decode($jsonString, true);

        $diff = (new \DateTime())->diff((new \DateTime())->setTimestamp($jsonData['timestamp']));

        return
            $diff->i
            +
            ($diff->h * 60)
            +
            ($diff->d * 24 * 60)
            > 60;
    }
}
