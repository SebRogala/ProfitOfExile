<?php

namespace App\Presentation\Controller;

use App\Application\Command\PriceRegistry\UpdateRegistry;
use App\Application\CommandBus;
use App\Infrastructure\Http\PoeNinjaHttpClient;
use App\Infrastructure\Http\TftHttpClient;
use Symfony\Bundle\FrameworkBundle\Controller\AbstractController;
use Symfony\Component\HttpFoundation\JsonResponse;
use Symfony\Component\HttpFoundation\Response;
use Symfony\Component\Routing\Annotation\Route;

class BaseVueController extends AbstractController
{
    #[Route("/app/",    name: "vue_home",    methods: ["GET"])]
    public function renderBaseTemplate(): Response
    {
        return $this->render('vue.html.twig');
    }


    #[Route("/test",    name: "test",    methods: ["GET"])]
    public function test(CommandBus $commandBus): Response
    {
//        $currencyResponse = $poeNinjaHttpClient->getCurrencyPrices();

//        return new JsonResponse($poeNinjaHttpClient->searchFor('divine-orb', $currencyResponse));


//        $res = $client->getBulkSetsPrices();

        $command = new UpdateRegistry();

        $commandBus->handle($command);

        return new JsonResponse();

//        return new JsonResponse($client->searchFor('Shaper Set', $res));
    }
}
