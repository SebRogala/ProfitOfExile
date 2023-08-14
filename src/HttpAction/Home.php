<?php

namespace App\HttpAction;

use App\Item\ItemPrice;
use Doctrine\ODM\MongoDB\DocumentManager;
use Symfony\Bundle\FrameworkBundle\Controller\AbstractController;
use Symfony\Component\HttpFoundation\JsonResponse;
use Symfony\Component\HttpFoundation\Response;
use Symfony\Component\Routing\Annotation\Route;

class Home extends AbstractController
{
    #[Route('/', name: 'app_home')]
    public function index(): Response
    {
        return $this->redirectToRoute('vue_home');
    }

    #[Route('/test', name: 'test')]
    public function test(DocumentManager $documentManager): Response
    {
        //$price = new ItemPrice(ChaosOrb::class, 0, 0);
        //$documentManager->persist($price);
        //$documentManager->flush();

//dd($documentManager->getRepository(ItemPrice::class)->findAll());
//dd($documentManager->getRepository(ItemPrice::class)->clear());

        //return new JsonResponse(['ok']);
        $qb = $documentManager->createQueryBuilder();

        $qb->remove(ItemPrice::class)->getQuery()->execute();

        //$documentManager->flush();

        return new JsonResponse(['ok']);
        return new JsonResponse();
    }
}
