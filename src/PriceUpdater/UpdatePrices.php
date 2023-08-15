<?php

namespace App\PriceUpdater;

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
use App\Item\ItemPrice\ItemPrice;
use App\Item\ItemPrice\ItemPriceRepository;
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

class UpdatePrices
{
    public function __construct(
        private PoeNinjaHttpClient $poeNinjaHttpClient,
        private TftHttpClient $tftHttpClient,
        private ItemPriceRepository $itemPriceRepository
    ) {
    }

    public function handle(bool $shouldForceUpdate = false): void
    {
        if (!$this->shouldUpdate($shouldForceUpdate)) {
            return;
        }

        $this->itemPriceRepository->removeAll();

        $prices = [];

        $divPrice = $this->poeNinjaHttpClient->searchFor('divine-orb')['chaosEquivalent'];

        $prices[] = new ItemPrice(ChaosOrb::class, 1, null);
        $prices[] = new ItemPrice(DivineOrb::class, $divPrice, null);
        $prices[] = new ItemPrice(
            OrbOfScouring::class,
            $this->poeNinjaHttpClient->searchFor('orb-of-scouring')['chaosEquivalent'],
            null
        );
        $prices[] = new ItemPrice(
            YellowLifeforce::class,
            $this->poeNinjaHttpClient->searchFor('vivid-crystallised-lifeforce')['receive']['value'],
            $this->tftHttpClient->searchFor('Vivid (Yellow)')['chaos'] / $this->tftHttpClient->searchFor(
                'Vivid (Yellow)'
            )['ratio']
        );
        $prices[] = new ItemPrice(
            BlueLifeforce::class,
            $this->poeNinjaHttpClient->searchFor('primal-crystallised-lifeforce')['receive']['value'],
            $this->tftHttpClient->searchFor('Primal (Blue)')['chaos'] / $this->tftHttpClient->searchFor(
                'Primal (Blue)'
            )['ratio']
        );
        $prices[] = new ItemPrice(
            PurpleLifeforce::class,
            $this->poeNinjaHttpClient->searchFor('wild-crystallised-lifeforce')['receive']['value'],
            $this->tftHttpClient->searchFor('Wild (Purple)')['chaos'] / $this->tftHttpClient->searchFor(
                'Wild (Purple)'
            )['ratio']
        );
        $prices[] = new ItemPrice(
            ShaperGuardianMap::class,
            $this->calculatePriceOfFour(
                $this->poeNinjaHttpClient->searchFor('forge-of-the-phoenix-map-t16-gen-17')['chaosValue'],
                $this->poeNinjaHttpClient->searchFor('lair-of-the-hydra-map-t16-gen-17')['chaosValue'],
                $this->poeNinjaHttpClient->searchFor('lair-of-the-hydra-map-t16-gen-17')['chaosValue'],
                $this->poeNinjaHttpClient->searchFor('pit-of-the-chimera-map-t16-gen-17')['chaosValue'],
            ),
            $this->tftHttpClient->searchFor('Shaper Maps')['chaos']
        );
        $prices[] = new ItemPrice(
            ElderGuardianMap::class,
            null,   //how to get that?
            $this->tftHttpClient->searchFor('Elder Maps')['chaos']
        );
        $prices[] = new ItemPrice(
            ShaperGuardianFragment::class,
            $this->calculatePriceOfFour(
                $this->poeNinjaHttpClient->searchFor('fragment-of-the-hydra')['chaosEquivalent'],
                $this->poeNinjaHttpClient->searchFor('fragment-of-the-minotaur')['chaosEquivalent'],
                $this->poeNinjaHttpClient->searchFor('fragment-of-the-chimera')['chaosEquivalent'],
                $this->poeNinjaHttpClient->searchFor('fragment-of-the-phoenix')['chaosEquivalent'],
            ),
            $this->tftHttpClient->searchFor('Shaper Set')['chaos'] / 4
        );
        $prices[] = new ItemPrice(
            ElderGuardianFragment::class,
            $this->calculatePriceOfFour(
                $this->poeNinjaHttpClient->searchFor('fragment-of-purification')['chaosEquivalent'],
                $this->poeNinjaHttpClient->searchFor('fragment-of-enslavement')['chaosEquivalent'],
                $this->poeNinjaHttpClient->searchFor('fragment-of-constriction')['chaosEquivalent'],
                $this->poeNinjaHttpClient->searchFor('fragment-of-eradication')['chaosEquivalent'],
            ),
            $this->tftHttpClient->searchFor('Elder Set')['chaos'] / 4
        );
        $prices[] = new ItemPrice(
            ShaperSet::class,
            $this->calculateSumPriceOfFour(
                $this->poeNinjaHttpClient->searchFor('fragment-of-the-hydra')['chaosEquivalent'],
                $this->poeNinjaHttpClient->searchFor('fragment-of-the-minotaur')['chaosEquivalent'],
                $this->poeNinjaHttpClient->searchFor('fragment-of-the-chimera')['chaosEquivalent'],
                $this->poeNinjaHttpClient->searchFor('fragment-of-the-phoenix')['chaosEquivalent'],
            ),
            $this->tftHttpClient->searchFor('Shaper Set')['chaos']
        );
        $prices[] = new ItemPrice(
            ElderSet::class,
            $this->calculateSumPriceOfFour(
                $this->poeNinjaHttpClient->searchFor('fragment-of-purification')['chaosEquivalent'],
                $this->poeNinjaHttpClient->searchFor('fragment-of-enslavement')['chaosEquivalent'],
                $this->poeNinjaHttpClient->searchFor('fragment-of-constriction')['chaosEquivalent'],
                $this->poeNinjaHttpClient->searchFor('fragment-of-eradication')['chaosEquivalent'],
            ),
            $this->tftHttpClient->searchFor('Elder Set')['chaos']
        );
        $prices[] = new ItemPrice(
            UberElderShaperFragment::class,
            ($this->poeNinjaHttpClient->searchFor('fragment-of-shape')['chaosEquivalent']
                + $this->poeNinjaHttpClient->searchFor('fragment-of-knowledge')['chaosEquivalent']) / 2,
            null
        );
        $prices[] = new ItemPrice(
            UberElderElderFragment::class,
            ($this->poeNinjaHttpClient->searchFor('fragment-of-emptiness')['chaosEquivalent']
                + $this->poeNinjaHttpClient->searchFor('fragment-of-terror')['chaosEquivalent']) / 2,
            null
        );
        $prices[] = new ItemPrice(
            UberElderSet::class,
            $this->calculateSumPriceOfFour(
                $this->poeNinjaHttpClient->searchFor('fragment-of-emptiness')['chaosEquivalent'],
                $this->poeNinjaHttpClient->searchFor('fragment-of-terror')['chaosEquivalent'],
                $this->poeNinjaHttpClient->searchFor('fragment-of-shape')['chaosEquivalent'],
                $this->poeNinjaHttpClient->searchFor('fragment-of-knowledge')['chaosEquivalent'],
            ),
            $this->tftHttpClient->searchFor('Uber Elder Set')['chaos']
        );
        $prices[] = new ItemPrice(
            TheFormed::class,
            $this->poeNinjaHttpClient->searchFor('mavens-invitation:-the-formed')['chaosValue'],
            $this->tftHttpClient->searchFor('The Formed')['chaos']
        );
        $prices[] = new ItemPrice(
            TheTwisted::class,
            $this->poeNinjaHttpClient->searchFor('mavens-invitation:-the-twisted')['chaosValue'],
            $this->tftHttpClient->searchFor('The Twisted')['chaos']
        );
        $prices[] = new ItemPrice(
            MavenSplinter::class,
            $this->poeNinjaHttpClient->searchFor('crescent-splinter')['chaosEquivalent'],
            null
        );
        $prices[] = new ItemPrice(
            MavenWrit::class,
            $this->poeNinjaHttpClient->searchFor('the-mavens-writ')['chaosEquivalent'],
            $this->tftHttpClient->searchFor('Maven\'s Writ')['chaos']
        );

        $this->itemPriceRepository->addMany($prices);
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
        if ($shouldForceUpdate) {
            return true;
        }

        return false;
        //$jsonString = file_get_contents($this->path);
        //$jsonData = json_decode($jsonString, true);

        //$diff = (new \DateTime())->diff((new \DateTime())->setTimestamp($jsonData['timestamp']));

        //return
        //    $diff->i
        //    +
        //    ($diff->h * 60)
        //    +
        //    ($diff->d * 24 * 60)
        //    > 60;
    }
}
